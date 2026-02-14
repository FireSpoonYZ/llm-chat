use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::middleware::{AppState, AuthUser};
use crate::db;
use crate::error::AppError;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation))
        .route("/{id}", get(get_conversation).put(update_conversation).delete(delete_conversation))
        .route("/{id}/messages", get(list_messages))
        .route("/{id}/mcp-servers", get(get_mcp_servers).put(set_mcp_servers))
}

#[derive(Serialize)]
pub struct ConversationResponse {
    pub id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<db::conversations::Conversation> for ConversationResponse {
    fn from(c: db::conversations::Conversation) -> Self {
        Self {
            id: c.id,
            title: c.title,
            provider: c.provider,
            model_name: c.model_name,
            system_prompt_override: c.system_prompt_override,
            deep_thinking: c.deep_thinking,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

async fn list_conversations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<ConversationResponse>>, AppError> {
    let convos = db::conversations::list_conversations(&state.db, &auth.user_id).await?;
    Ok(Json(convos.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
    pub system_prompt_override: Option<String>,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub deep_thinking: Option<bool>,
}

async fn create_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationResponse>), AppError> {
    let title = req.title.unwrap_or_else(|| "New Conversation".into());
    let conv = db::conversations::create_conversation(
        &state.db,
        &auth.user_id,
        &title,
        req.system_prompt_override.as_deref(),
        req.provider.as_deref(),
        req.model_name.as_deref(),
        req.deep_thinking.unwrap_or(false),
    ).await?;

    // Create workspace directory
    let workspace_dir = format!("data/conversations/{}", conv.id);
    let _ = tokio::fs::create_dir_all(&workspace_dir).await;

    Ok((StatusCode::CREATED, Json(conv.into())))
}

async fn get_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ConversationResponse>, AppError> {
    let conv = db::conversations::get_conversation(&state.db, &id, &auth.user_id).await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(conv.into()))
}

#[derive(Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: Option<bool>,
}

async fn update_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<ConversationResponse>, AppError> {
    let existing = db::conversations::get_conversation(&state.db, &id, &auth.user_id).await?
        .ok_or(AppError::NotFound)?;

    let title = req.title.as_deref().unwrap_or(&existing.title);
    let provider = match req.provider.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.provider.as_deref(),
    };
    let model_name = match req.model_name.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.model_name.as_deref(),
    };
    let system_prompt = match req.system_prompt_override.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.system_prompt_override.as_deref(),
    };
    let deep_thinking = req.deep_thinking.unwrap_or(existing.deep_thinking);

    let conv = db::conversations::update_conversation(
        &state.db,
        &id,
        &auth.user_id,
        title,
        provider,
        model_name,
        system_prompt,
        deep_thinking,
    ).await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(conv.into()))
}

async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if db::conversations::delete_conversation(&state.db, &id, &auth.user_id).await? {
        let workspace_dir = format!("data/conversations/{}", id);
        let _ = tokio::fs::remove_dir_all(&workspace_dir).await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

#[derive(Deserialize)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct MessagesResponse {
    pub messages: Vec<MessageResponse>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub token_count: Option<i64>,
    pub created_at: String,
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<MessagesResponse>, AppError> {
    // Verify conversation belongs to user
    db::conversations::get_conversation(&state.db, &id, &auth.user_id).await?
        .ok_or(AppError::NotFound)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let messages = db::messages::list_messages(&state.db, &id, limit, offset).await?;
    let total = db::messages::count_messages(&state.db, &id).await?;

    Ok(Json(MessagesResponse {
        messages: messages
            .into_iter()
            .map(|m| MessageResponse {
                id: m.id,
                role: m.role,
                content: m.content,
                tool_calls: m.tool_calls,
                tool_call_id: m.tool_call_id,
                token_count: m.token_count,
                created_at: m.created_at,
            })
            .collect(),
        total,
    }))
}

#[derive(Serialize)]
pub struct McpServerResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub transport: String,
    pub is_enabled: bool,
}

async fn get_mcp_servers(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<McpServerResponse>>, AppError> {
    db::conversations::get_conversation(&state.db, &id, &auth.user_id).await?
        .ok_or(AppError::NotFound)?;

    let servers = db::mcp_servers::get_conversation_mcp_servers(&state.db, &id).await?;
    Ok(Json(
        servers
            .into_iter()
            .map(|s| McpServerResponse {
                id: s.id,
                name: s.name,
                description: s.description,
                transport: s.transport,
                is_enabled: s.is_enabled,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct SetMcpServersRequest {
    pub server_ids: Vec<String>,
}

async fn set_mcp_servers(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SetMcpServersRequest>,
) -> Result<StatusCode, AppError> {
    db::conversations::get_conversation(&state.db, &id, &auth.user_id).await?
        .ok_or(AppError::NotFound)?;

    db::mcp_servers::set_conversation_mcp_servers(&state.db, &id, &req.server_ids).await?;

    Ok(StatusCode::OK)
}
