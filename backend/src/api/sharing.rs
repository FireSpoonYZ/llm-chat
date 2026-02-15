use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use rand::Rng;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

use crate::auth::middleware::{AppState, AuthUser};
use crate::db;
use crate::error::AppError;

use super::conversations::PaginationParams;
use super::files::{parse_range, resolve_safe_path};

// ── Authenticated endpoints (share management) ──

pub fn share_management_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{id}/share", post(create_share).delete(revoke_share))
}

// ── Public endpoints (no auth) ──

pub fn shared_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{share_token}", get(get_shared_conversation))
        .route("/{share_token}/messages", get(get_shared_messages))
        .route("/{share_token}/files/view", get(view_shared_file))
}

#[derive(Serialize)]
struct ShareResponse {
    share_token: String,
    share_url: String,
}

async fn create_share(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ShareResponse>, AppError> {
    // Check if already shared
    let conv = db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if let Some(existing_token) = conv.share_token {
        return Ok(Json(ShareResponse {
            share_url: format!("/share/{}", existing_token),
            share_token: existing_token,
        }));
    }

    let token = generate_share_token();
    let result = db::conversations::set_share_token(&state.db, &id, &auth.user_id, &token)
        .await?;

    // If set_share_token returned None, another request may have set it concurrently.
    // Re-read the conversation to get the winning token.
    let conv = match result {
        Some(c) => c,
        None => db::conversations::get_conversation(&state.db, &id, &auth.user_id)
            .await?
            .ok_or(AppError::NotFound)?,
    };

    let final_token = conv.share_token.ok_or(AppError::NotFound)?;
    Ok(Json(ShareResponse {
        share_url: format!("/share/{}", final_token),
        share_token: final_token,
    }))
}

async fn revoke_share(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let removed = db::conversations::remove_share_token(&state.db, &id, &auth.user_id).await?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

// ── Public shared conversation endpoints ──

#[derive(Serialize)]
struct SharedConversationResponse {
    title: String,
    model_name: Option<String>,
    created_at: String,
    updated_at: String,
}

async fn get_shared_conversation(
    State(state): State<Arc<AppState>>,
    Path(share_token): Path<String>,
) -> Result<Json<SharedConversationResponse>, AppError> {
    let conv = db::conversations::get_conversation_by_share_token(&state.db, &share_token)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(SharedConversationResponse {
        title: conv.title,
        model_name: conv.model_name,
        created_at: conv.created_at,
        updated_at: conv.updated_at,
    }))
}

#[derive(Serialize)]
struct SharedMessageResponse {
    id: String,
    role: String,
    content: String,
    tool_calls: Option<String>,
    tool_call_id: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
struct SharedMessagesResponse {
    messages: Vec<SharedMessageResponse>,
    total: i64,
}

async fn get_shared_messages(
    State(state): State<Arc<AppState>>,
    Path(share_token): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<SharedMessagesResponse>, AppError> {
    let conv = db::conversations::get_conversation_by_share_token(&state.db, &share_token)
        .await?
        .ok_or(AppError::NotFound)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let messages = db::messages::list_messages(&state.db, &conv.id, limit, offset).await?;
    let total = db::messages::count_messages(&state.db, &conv.id).await?;

    Ok(Json(SharedMessagesResponse {
        messages: messages
            .into_iter()
            .map(|m| SharedMessageResponse {
                id: m.id,
                role: m.role,
                content: m.content,
                tool_calls: m.tool_calls,
                tool_call_id: m.tool_call_id,
                created_at: m.created_at,
            })
            .collect(),
        total,
    }))
}

#[derive(serde::Deserialize)]
struct FileViewQuery {
    path: String,
}

async fn view_shared_file(
    State(state): State<Arc<AppState>>,
    Path(share_token): Path<String>,
    Query(query): Query<FileViewQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let conv = db::conversations::get_conversation_by_share_token(&state.db, &share_token)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace_root = PathBuf::from(format!("data/conversations/{}", conv.id));
    let file_path = resolve_safe_path(&workspace_root, &query.path)
        .ok_or_else(|| AppError::Forbidden("Path traversal denied".into()))?;

    if !file_path.is_file() {
        return Err(AppError::NotFound);
    }

    let metadata = tokio::fs::metadata(&file_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let file_size = metadata.len();

    let mime = mime_guess::from_path(&file_path)
        .first_raw()
        .unwrap_or("application/octet-stream");

    // Range support
    if let Some(range_header) = headers.get(header::RANGE) {
        let range_str = range_header
            .to_str()
            .map_err(|_| AppError::BadRequest("Invalid Range header".into()))?;

        if let Some((start, end)) = parse_range(range_str, file_size) {
            let length = end - start + 1;
            let file = tokio::fs::File::open(&file_path)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            use tokio::io::AsyncSeekExt;
            let mut file = file;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let limited = file.take(length);
            let stream = ReaderStream::new(limited);
            let body = Body::from_stream(stream);

            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .header(header::CONTENT_LENGTH, length.to_string())
                .body(body)
                .map_err(|e| AppError::Internal(e.to_string()));
        }
    }

    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .header(header::CACHE_CONTROL, "private, max-age=3600, immutable")
        .body(body)
        .map_err(|e| AppError::Internal(e.to_string()))
}

fn generate_share_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().r#gen();
    hex::encode(bytes)
}
