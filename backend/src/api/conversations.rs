use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, sync::Arc};

use crate::auth::middleware::{AppState, AuthUser};
use crate::db;
use crate::error::AppError;

const DEFAULT_THINKING_BUDGET: i64 = 128000;
const MIN_THINKING_BUDGET: i64 = 1024;
const MAX_THINKING_BUDGET: i64 = 1_000_000;

fn validate_budget(field_name: &str, budget: i64) -> Result<(), AppError> {
    if !(MIN_THINKING_BUDGET..=MAX_THINKING_BUDGET).contains(&budget) {
        return Err(AppError::BadRequest(format!(
            "{field_name} must be between {MIN_THINKING_BUDGET} and {MAX_THINKING_BUDGET}"
        )));
    }
    Ok(())
}

fn validate_optional_budget(field_name: &str, budget: Option<i64>) -> Result<(), AppError> {
    if let Some(value) = budget {
        validate_budget(field_name, value)?;
    }
    Ok(())
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation))
        .route(
            "/{id}",
            get(get_conversation)
                .put(update_conversation)
                .delete(delete_conversation),
        )
        .route("/{id}/messages", get(list_messages))
        .route(
            "/{id}/mcp-servers",
            get(get_mcp_servers).put(set_mcp_servers),
        )
}

#[derive(Serialize)]
pub struct ConversationResponse {
    pub id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider: Option<String>,
    pub subagent_model: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: bool,
    pub created_at: String,
    pub updated_at: String,
    pub image_provider: Option<String>,
    pub image_model: Option<String>,
    pub share_token: Option<String>,
    pub thinking_budget: Option<i64>,
    pub subagent_thinking_budget: Option<i64>,
}

impl From<db::conversations::Conversation> for ConversationResponse {
    fn from(c: db::conversations::Conversation) -> Self {
        Self {
            id: c.id,
            title: c.title,
            provider: c.provider,
            model_name: c.model_name,
            subagent_provider: c.subagent_provider,
            subagent_model: c.subagent_model,
            system_prompt_override: c.system_prompt_override,
            deep_thinking: c.deep_thinking,
            created_at: c.created_at,
            updated_at: c.updated_at,
            image_provider: c.image_provider,
            image_model: c.image_model,
            share_token: c.share_token,
            thinking_budget: c.thinking_budget,
            subagent_thinking_budget: c.subagent_thinking_budget,
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
    pub subagent_provider: Option<String>,
    pub subagent_model: Option<String>,
    pub deep_thinking: Option<bool>,
    pub image_provider: Option<String>,
    pub image_model: Option<String>,
    pub thinking_budget: Option<i64>,
    pub subagent_thinking_budget: Option<i64>,
}

async fn create_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationResponse>), AppError> {
    validate_optional_budget("thinking_budget", req.thinking_budget)?;
    validate_optional_budget("subagent_thinking_budget", req.subagent_thinking_budget)?;

    let title = req.title.unwrap_or_else(|| "New Conversation".into());
    let subagent_provider = req.subagent_provider.as_deref().or(req.provider.as_deref());
    let subagent_model = req.subagent_model.as_deref().or(req.model_name.as_deref());
    let thinking_budget = req.thinking_budget.unwrap_or(DEFAULT_THINKING_BUDGET);
    let subagent_thinking_budget = req.subagent_thinking_budget.unwrap_or(thinking_budget);
    let conv = db::conversations::create_conversation_with_subagent(
        &state.db,
        &auth.user_id,
        &title,
        req.system_prompt_override.as_deref(),
        req.provider.as_deref(),
        req.model_name.as_deref(),
        subagent_provider,
        subagent_model,
        req.deep_thinking.unwrap_or(true),
        req.image_provider.as_deref(),
        req.image_model.as_deref(),
        Some(thinking_budget),
        Some(subagent_thinking_budget),
    )
    .await?;

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
    let conv = db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(conv.into()))
}

#[derive(Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider: Option<String>,
    pub subagent_model: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: Option<bool>,
    pub image_provider: Option<String>,
    pub image_model: Option<String>,
    pub thinking_budget: Option<i64>,
    pub subagent_thinking_budget: Option<i64>,
}

async fn update_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<Json<ConversationResponse>, AppError> {
    validate_optional_budget("thinking_budget", req.thinking_budget)?;
    validate_optional_budget("subagent_thinking_budget", req.subagent_thinking_budget)?;

    let existing = db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
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
    let subagent_provider = match req.subagent_provider.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.subagent_provider.as_deref(),
    };
    let subagent_model = match req.subagent_model.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.subagent_model.as_deref(),
    };
    let system_prompt = match req.system_prompt_override.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.system_prompt_override.as_deref(),
    };
    let deep_thinking = req.deep_thinking.unwrap_or(existing.deep_thinking);
    let thinking_budget = req.thinking_budget.or(existing.thinking_budget);
    let subagent_thinking_budget = req
        .subagent_thinking_budget
        .or(existing.subagent_thinking_budget);
    let image_provider = match req.image_provider.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.image_provider.as_deref(),
    };
    let image_model = match req.image_model.as_deref() {
        Some("") => None,
        Some(v) => Some(v),
        None => existing.image_model.as_deref(),
    };

    // If provider or model changed, stop the running container so it
    // re-initialises with the new config on the next message.
    let provider_changed = provider != existing.provider.as_deref();
    let model_changed = model_name != existing.model_name.as_deref();
    let subagent_provider_changed = subagent_provider != existing.subagent_provider.as_deref();
    let subagent_model_changed = subagent_model != existing.subagent_model.as_deref();
    let image_provider_changed = image_provider != existing.image_provider.as_deref();
    let image_model_changed = image_model != existing.image_model.as_deref();
    if provider_changed
        || model_changed
        || subagent_provider_changed
        || subagent_model_changed
        || image_provider_changed
        || image_model_changed
    {
        let _ = state.docker_manager.stop_container(&id).await;
        state.ws_state.remove_container(&id).await;
    }

    let conv = db::conversations::update_conversation_with_subagent(
        &state.db,
        &id,
        &auth.user_id,
        title,
        provider,
        model_name,
        subagent_provider,
        subagent_model,
        system_prompt,
        deep_thinking,
        image_provider,
        image_model,
        thinking_budget,
        subagent_thinking_budget,
    )
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(conv.into()))
}

async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if let Err(e) = state.docker_manager.stop_container(&id).await {
        tracing::warn!("Failed to stop container for conversation {}: {}", id, e);
    }
    state.ws_state.remove_container(&id).await;
    let _ = state.ws_state.take_pending_message(&id).await;

    let workspace_dir = format!("data/conversations/{}", id);
    match tokio::fs::remove_dir_all(&workspace_dir).await {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => {
            tracing::error!(
                "Failed to remove workspace for conversation {} at {}: {}",
                id,
                workspace_dir,
                e
            );
            return Err(AppError::Internal(
                "failed to delete conversation workspace".into(),
            ));
        }
    };

    if db::conversations::delete_conversation(&state.db, &id, &auth.user_id).await? {
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
    pub parts: Vec<MessagePartResponse>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub token_count: Option<i64>,
    pub created_at: String,
}

#[derive(Serialize, Clone)]
pub struct MessagePartResponse {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
    pub json_payload: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
    pub seq: Option<i64>,
}

fn legacy_parts_from_message(m: &db::messages::Message) -> Vec<MessagePartResponse> {
    db::messages_v2::legacy_message_to_parts(m)
        .into_iter()
        .enumerate()
        .map(|(idx, p)| MessagePartResponse {
            part_type: p.part_type,
            text: p.text,
            json_payload: p
                .json_payload
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
            tool_call_id: p.tool_call_id,
            seq: Some(idx as i64),
        })
        .collect()
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<MessagesResponse>, AppError> {
    // Verify conversation belongs to user
    db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let messages = db::messages::list_messages(&state.db, &id, limit, offset).await?;
    let total = db::messages::count_messages(&state.db, &id).await?;
    let message_ids = messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    let existing_v2_ids =
        db::messages_v2::list_existing_message_v2_ids(&state.db, &message_ids).await?;
    let parts_by_message_id =
        db::messages_v2::list_message_parts_for_messages(&state.db, &message_ids).await?;

    Ok(Json(MessagesResponse {
        messages: {
            let mut out: Vec<MessageResponse> = Vec::with_capacity(messages.len());
            for m in messages {
                let parts = if existing_v2_ids.contains(&m.id) {
                    parts_by_message_id
                        .get(&m.id)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| MessagePartResponse {
                            part_type: p.part_type,
                            text: p.text,
                            json_payload: p
                                .json_payload
                                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                            tool_call_id: p.tool_call_id,
                            seq: Some(p.seq),
                        })
                        .collect()
                } else {
                    legacy_parts_from_message(&m)
                };
                out.push(MessageResponse {
                    id: m.id,
                    role: m.role,
                    content: m.content,
                    parts,
                    tool_calls: m.tool_calls,
                    tool_call_id: m.tool_call_id,
                    token_count: m.token_count,
                    created_at: m.created_at,
                });
            }
            out
        },
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
    db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
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
    db::conversations::get_conversation(&state.db, &id, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    db::mcp_servers::set_conversation_mcp_servers(&state.db, &id, &req.server_ids).await?;

    Ok(StatusCode::OK)
}
