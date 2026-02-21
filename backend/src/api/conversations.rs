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

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

fn parse_models_json(json_str: Option<&str>) -> Vec<String> {
    json_str
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

fn ensure_provider_has_model(
    providers: &[db::providers::UserProvider],
    provider_id: &str,
    model_name: &str,
    use_image_models: bool,
) -> Result<(), AppError> {
    let provider = providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| {
            AppError::BadRequest(format!("Provider id '{provider_id}' does not exist"))
        })?;

    let models = if use_image_models {
        parse_models_json(provider.image_models.as_deref())
    } else {
        parse_models_json(provider.models.as_deref())
    };
    if models.iter().any(|m| m == model_name) {
        return Ok(());
    }
    let kind = if use_image_models {
        "image model"
    } else {
        "model"
    };
    Err(AppError::BadRequest(format!(
        "{kind} '{model_name}' is not available for provider id '{provider_id}'"
    )))
}

struct ValidatedConversationModels {
    provider_id: String,
    model_name: String,
    subagent_provider_id: String,
    subagent_model: String,
    image_provider_id: Option<String>,
    image_model: Option<String>,
}

fn validate_conversation_models(
    providers: &[db::providers::UserProvider],
    provider_id: Option<String>,
    model_name: Option<String>,
    subagent_provider_id: Option<String>,
    subagent_model: Option<String>,
    image_provider_id: Option<String>,
    image_model: Option<String>,
) -> Result<ValidatedConversationModels, AppError> {
    let provider_id =
        provider_id.ok_or_else(|| AppError::BadRequest("provider_id is required".into()))?;
    let model_name =
        model_name.ok_or_else(|| AppError::BadRequest("model_name is required".into()))?;
    let subagent_provider_id = subagent_provider_id
        .ok_or_else(|| AppError::BadRequest("subagent_provider_id is required".into()))?;
    let subagent_model =
        subagent_model.ok_or_else(|| AppError::BadRequest("subagent_model is required".into()))?;

    if image_provider_id.is_some() ^ image_model.is_some() {
        return Err(AppError::BadRequest(
            "image_provider_id and image_model must both be set or both be empty".into(),
        ));
    }

    ensure_provider_has_model(providers, &provider_id, &model_name, false)?;
    ensure_provider_has_model(providers, &subagent_provider_id, &subagent_model, false)?;
    if let (Some(provider_id), Some(model_name)) =
        (image_provider_id.as_deref(), image_model.as_deref())
    {
        ensure_provider_has_model(providers, provider_id, model_name, true)?;
    }

    Ok(ValidatedConversationModels {
        provider_id,
        model_name,
        subagent_provider_id,
        subagent_model,
        image_provider_id,
        image_model,
    })
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
    pub provider_id: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: bool,
    pub created_at: String,
    pub updated_at: String,
    pub image_provider_id: Option<String>,
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
            provider_id: c.provider_id,
            model_name: c.model_name,
            subagent_provider_id: c.subagent_provider_id,
            subagent_model: c.subagent_model,
            system_prompt_override: c.system_prompt_override,
            deep_thinking: c.deep_thinking,
            created_at: c.created_at,
            updated_at: c.updated_at,
            image_provider_id: c.image_provider_id,
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
    pub provider_id: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model: Option<String>,
    pub deep_thinking: Option<bool>,
    pub image_provider_id: Option<String>,
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
    let providers = db::providers::list_providers(&state.db, &auth.user_id).await?;
    let validated_models = validate_conversation_models(
        &providers,
        normalize_optional_string(req.provider_id.as_deref()),
        normalize_optional_string(req.model_name.as_deref()),
        normalize_optional_string(req.subagent_provider_id.as_deref()),
        normalize_optional_string(req.subagent_model.as_deref()),
        normalize_optional_string(req.image_provider_id.as_deref()),
        normalize_optional_string(req.image_model.as_deref()),
    )?;

    let thinking_budget = req.thinking_budget.unwrap_or(DEFAULT_THINKING_BUDGET);
    let subagent_thinking_budget = req.subagent_thinking_budget.unwrap_or(thinking_budget);
    let conv = db::conversations::create_conversation_with_subagent(
        &state.db,
        &auth.user_id,
        &title,
        req.system_prompt_override.as_deref(),
        Some(validated_models.provider_id.as_str()),
        Some(validated_models.model_name.as_str()),
        Some(validated_models.subagent_provider_id.as_str()),
        Some(validated_models.subagent_model.as_str()),
        req.deep_thinking.unwrap_or(true),
        validated_models.image_provider_id.as_deref(),
        validated_models.image_model.as_deref(),
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
    pub provider_id: Option<String>,
    pub model_name: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model: Option<String>,
    pub system_prompt_override: Option<String>,
    pub deep_thinking: Option<bool>,
    pub image_provider_id: Option<String>,
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
    let provider_id = match req.provider_id.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.provider_id.as_deref()),
    };
    let model_name = match req.model_name.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.model_name.as_deref()),
    };
    let subagent_provider_id = match req.subagent_provider_id.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.subagent_provider_id.as_deref()),
    };
    let subagent_model = match req.subagent_model.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.subagent_model.as_deref()),
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
    let image_provider_id = match req.image_provider_id.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.image_provider_id.as_deref()),
    };
    let image_model = match req.image_model.as_deref() {
        Some(value) => normalize_optional_string(Some(value)),
        None => normalize_optional_string(existing.image_model.as_deref()),
    };
    let providers = db::providers::list_providers(&state.db, &auth.user_id).await?;
    let validated_models = validate_conversation_models(
        &providers,
        provider_id,
        model_name,
        subagent_provider_id,
        subagent_model,
        image_provider_id,
        image_model,
    )?;

    // If provider_id or model changed, stop the running container so it
    // re-initialises with the new config on the next message.
    let provider_changed =
        Some(validated_models.provider_id.as_str()) != existing.provider_id.as_deref();
    let model_changed =
        Some(validated_models.model_name.as_str()) != existing.model_name.as_deref();
    let subagent_provider_changed = Some(validated_models.subagent_provider_id.as_str())
        != existing.subagent_provider_id.as_deref();
    let subagent_model_changed =
        Some(validated_models.subagent_model.as_str()) != existing.subagent_model.as_deref();
    let image_provider_changed =
        validated_models.image_provider_id.as_deref() != existing.image_provider_id.as_deref();
    let image_model_changed =
        validated_models.image_model.as_deref() != existing.image_model.as_deref();
    if provider_changed
        || model_changed
        || subagent_provider_changed
        || subagent_model_changed
        || image_provider_changed
        || image_model_changed
    {
        state
            .ws_state
            .send_to_client(
                &auth.user_id,
                &id,
                &serde_json::json!({
                    "type": "container_status",
                    "conversation_id": &id,
                    "status": "restarting",
                    "reason": "model_switch",
                    "message": "Switching model. Restarting container..."
                })
                .to_string(),
            )
            .await;
        let _ = state.docker_manager.stop_container(&id).await;
        state.ws_state.remove_container(&id).await;
    }

    let conv = db::conversations::update_conversation_with_subagent(
        &state.db,
        &id,
        &auth.user_id,
        title,
        Some(validated_models.provider_id.as_str()),
        Some(validated_models.model_name.as_str()),
        Some(validated_models.subagent_provider_id.as_str()),
        Some(validated_models.subagent_model.as_str()),
        system_prompt,
        deep_thinking,
        validated_models.image_provider_id.as_deref(),
        validated_models.image_model.as_deref(),
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
