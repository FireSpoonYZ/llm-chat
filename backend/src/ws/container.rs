use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::WsState;
use super::messages::ContainerMessage;
use crate::auth;
use crate::auth::middleware::AppState;
use crate::db;

const INIT_PAYLOAD_WARN_BYTES: usize = 1_000_000;

fn non_empty_str(value: Option<&str>) -> Option<&str> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn parse_models_json(value: Option<&str>) -> Vec<String> {
    value
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

#[derive(Debug)]
struct ResolvedConversationProviders {
    chat_provider: db::providers::UserProvider,
    chat_model: String,
    subagent_provider: db::providers::UserProvider,
    subagent_model: String,
    image_provider: Option<db::providers::UserProvider>,
    image_model: Option<String>,
}

fn resolve_provider<'a>(
    providers: &'a [db::providers::UserProvider],
    provider_id: &str,
) -> Result<&'a db::providers::UserProvider, String> {
    providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Provider id '{provider_id}' does not exist"))
}

fn ensure_provider_has_model(
    provider: &db::providers::UserProvider,
    model_name: &str,
    use_image_models: bool,
) -> Result<(), String> {
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
    Err(format!(
        "{kind} '{model_name}' is not available for provider id '{}'",
        provider.id
    ))
}

fn resolve_conversation_providers(
    conv: &db::conversations::Conversation,
    providers: &[db::providers::UserProvider],
) -> Result<ResolvedConversationProviders, String> {
    let provider_id = non_empty_str(conv.provider_id.as_deref())
        .ok_or_else(|| "provider_id is required for this conversation".to_string())?;
    let model_name = non_empty_str(conv.model_name.as_deref())
        .ok_or_else(|| "model_name is required for this conversation".to_string())?;
    let subagent_provider_id = non_empty_str(conv.subagent_provider_id.as_deref())
        .ok_or_else(|| "subagent_provider_id is required for this conversation".to_string())?;
    let subagent_model_name = non_empty_str(conv.subagent_model.as_deref())
        .ok_or_else(|| "subagent_model is required for this conversation".to_string())?;

    let chat_provider = resolve_provider(providers, provider_id)?;
    ensure_provider_has_model(chat_provider, model_name, false)?;

    let subagent_provider = resolve_provider(providers, subagent_provider_id)?;
    ensure_provider_has_model(subagent_provider, subagent_model_name, false)?;

    let image_provider_id = non_empty_str(conv.image_provider_id.as_deref());
    let image_model_name = non_empty_str(conv.image_model.as_deref());
    if image_provider_id.is_some() ^ image_model_name.is_some() {
        return Err(
            "image_provider_id and image_model must both be set or both be empty".to_string(),
        );
    }

    let image_provider = match image_provider_id {
        Some(provider_id) => {
            let provider = resolve_provider(providers, provider_id)?;
            let model_name = image_model_name.unwrap_or_default();
            ensure_provider_has_model(provider, model_name, true)?;
            Some(provider.clone())
        }
        None => None,
    };

    Ok(ResolvedConversationProviders {
        chat_provider: chat_provider.clone(),
        chat_model: model_name.to_string(),
        subagent_provider: subagent_provider.clone(),
        subagent_model: subagent_model_name.to_string(),
        image_provider,
        image_model: image_model_name.map(ToString::to_string),
    })
}

async fn fail_container_init(
    state: &Arc<AppState>,
    ws_state: &Arc<WsState>,
    user_id: &str,
    conversation_id: &str,
    code: &str,
    message: &str,
) {
    ws_state
        .send_to_client(
            user_id,
            conversation_id,
            &serde_json::json!({
                "type": "error",
                "conversation_id": conversation_id,
                "code": code,
                "message": message
            })
            .to_string(),
        )
        .await;

    let _ = ws_state.take_pending_message(conversation_id).await;
    let _ = state.docker_manager.stop_container(conversation_id).await;
    ws_state.remove_container(conversation_id).await;
    let reason = if code == "conversation_model_config_invalid" {
        "invalid_model_config"
    } else {
        "init_failed"
    };
    let status_message = if code == "conversation_model_config_invalid" {
        "Conversation model config is invalid. Please update model settings and retry."
    } else {
        "Container initialization failed. Please retry."
    };
    ws_state
        .send_to_client(
            user_id,
            conversation_id,
            &serde_json::json!({
                "type": "container_status",
                "conversation_id": conversation_id,
                "status": "disconnected",
                "reason": reason,
                "message": status_message
            })
            .to_string(),
        )
        .await;
}

#[derive(serde::Deserialize)]
pub struct ContainerWsQuery {
    pub token: String,
}

pub async fn container_ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<ContainerWsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match auth::verify_container_token(&query.token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

    let ws_state = state.ws_state.clone();

    ws.on_upgrade(move |socket| {
        handle_container_ws(socket, claims.sub, claims.user_id, state, ws_state)
    })
}

async fn handle_container_ws(
    socket: WebSocket,
    conversation_id: String,
    user_id: String,
    state: Arc<AppState>,
    ws_state: Arc<WsState>,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(super::WS_CHANNEL_CAPACITY);

    let container_gen = ws_state.add_container(&conversation_id, tx.clone()).await;

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    ws_state
        .send_to_client(
            &user_id,
            &conversation_id,
            &serde_json::json!({
                "type": "container_status",
                "conversation_id": conversation_id,
                "status": "connected",
                "reason": "ready",
                "message": "Container connected"
            })
            .to_string(),
        )
        .await;

    while let Some(Ok(msg)) = ws_stream.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let container_msg: ContainerMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    conversation_id = %conversation_id,
                    error = %e,
                    text = %&text[..text.len().min(200)],
                    "Failed to parse container message"
                );
                continue;
            }
        };

        // Refresh activity timestamp so the idle-cleanup task doesn't kill active containers.
        state.docker_manager.touch_activity(&conversation_id).await;

        match container_msg {
            ContainerMessage::Ready => {
                tracing::info!("Container ready for conversation {}", conversation_id);
                if let Some(conv) =
                    db::conversations::get_conversation(&state.db, &conversation_id, &user_id)
                        .await
                        .ok()
                        .flatten()
                {
                    let providers = match db::providers::list_providers(&state.db, &user_id).await {
                        Ok(providers) => providers,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conversation_id,
                                error = %e,
                                "Failed to load providers for container init"
                            );
                            fail_container_init(
                                &state,
                                &ws_state,
                                &user_id,
                                &conversation_id,
                                "container_init_failed",
                                "Failed to load providers. Please retry.",
                            )
                            .await;
                            break;
                        }
                    };
                    let resolved = match resolve_conversation_providers(&conv, &providers) {
                        Ok(resolved) => resolved,
                        Err(message) => {
                            tracing::warn!(
                                conversation_id = %conversation_id,
                                message = %message,
                                "Conversation model config is invalid for container init"
                            );
                            fail_container_init(
                                &state,
                                &ws_state,
                                &user_id,
                                &conversation_id,
                                "conversation_model_config_invalid",
                                &message,
                            )
                            .await;
                            break;
                        }
                    };

                    let messages = db::messages::list_messages(
                        &state.db,
                        &conversation_id,
                        super::CONTAINER_INIT_HISTORY_LIMIT,
                        0,
                    )
                    .await
                    .unwrap_or_default();

                    // Check if last message is from user — it will be resent separately
                    let needs_resend = messages.last().is_some_and(|m| m.role == "user");
                    let history_messages = if needs_resend {
                        &messages[..messages.len() - 1]
                    } else {
                        &messages[..]
                    };

                    let history: Vec<serde_json::Value> = history_messages
                        .iter()
                        .map(|m| {
                            let mut entry = serde_json::json!({
                                "role": m.role,
                                "content": m.content,
                            });
                            if let Some(ref tc) = m.tool_calls
                                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(tc)
                                && parsed.is_array()
                            {
                                entry["tool_calls"] = parsed;
                            }
                            entry
                        })
                        .collect();
                    let history_parts =
                        build_history_parts_for_init(&state.db, history_messages).await;

                    let mcp_servers =
                        db::mcp_servers::get_conversation_mcp_servers(&state.db, &conversation_id)
                            .await
                            .unwrap_or_default();
                    let mcp_configs: Vec<serde_json::Value> = mcp_servers
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "name": s.name,
                                "transport": s.transport,
                                "command": s.command,
                                "args": s.args,
                                "url": s.url,
                                "env_vars": s.env_vars,
                                "read_only_overrides": s.read_only_overrides,
                            })
                        })
                        .collect();

                    let api_key = match crate::crypto::decrypt(
                        &resolved.chat_provider.api_key_encrypted,
                        &state.config.encryption_key,
                    ) {
                        Ok(key) => key,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conversation_id,
                                provider_id = %resolved.chat_provider.id,
                                error = %e,
                                "Failed to decrypt chat provider API key"
                            );
                            fail_container_init(
                                &state,
                                &ws_state,
                                &user_id,
                                &conversation_id,
                                "decrypt_failed",
                                "Failed to decrypt chat provider API key. Please re-save provider settings.",
                            )
                            .await;
                            break;
                        }
                    };

                    let subagent_api_key = match crate::crypto::decrypt(
                        &resolved.subagent_provider.api_key_encrypted,
                        &state.config.encryption_key,
                    ) {
                        Ok(key) => key,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conversation_id,
                                provider_id = %resolved.subagent_provider.id,
                                error = %e,
                                "Failed to decrypt subagent provider API key"
                            );
                            fail_container_init(
                                &state,
                                &ws_state,
                                &user_id,
                                &conversation_id,
                                "decrypt_failed",
                                "Failed to decrypt subagent provider API key. Please re-save provider settings.",
                            )
                            .await;
                            break;
                        }
                    };

                    let image_api_key = if let Some(image_provider) =
                        resolved.image_provider.as_ref()
                    {
                        match crate::crypto::decrypt(
                            &image_provider.api_key_encrypted,
                            &state.config.encryption_key,
                        ) {
                            Ok(key) => key,
                            Err(e) => {
                                tracing::error!(
                                    conversation_id = %conversation_id,
                                    provider_id = %image_provider.id,
                                    error = %e,
                                    "Failed to decrypt image provider API key"
                                );
                                fail_container_init(
                                    &state,
                                    &ws_state,
                                    &user_id,
                                    &conversation_id,
                                    "decrypt_failed",
                                    "Failed to decrypt image provider API key. Please re-save provider settings.",
                                )
                                .await;
                                break;
                            }
                        }
                    } else {
                        String::new()
                    };

                    let image_provider_type = resolved
                        .image_provider
                        .as_ref()
                        .map(|p| p.provider.clone())
                        .unwrap_or_default();
                    let image_endpoint_url = resolved
                        .image_provider
                        .as_ref()
                        .and_then(|p| p.endpoint_url.clone());
                    let chat_provider_type = resolved.chat_provider.provider.clone();
                    let chat_endpoint_url = resolved.chat_provider.endpoint_url.clone();
                    let subagent_provider_type = resolved.subagent_provider.provider.clone();
                    let subagent_endpoint_url = resolved.subagent_provider.endpoint_url.clone();
                    let chat_model = resolved.chat_model.clone();
                    let subagent_model = resolved.subagent_model.clone();
                    let image_model = resolved.image_model.clone();

                    let init_msg = serde_json::json!({
                        "type": "init",
                        "conversation_id": conversation_id,
                        "provider": chat_provider_type,
                        "model": chat_model,
                        "api_key": api_key,
                        "endpoint_url": chat_endpoint_url,
                        "subagent_provider": subagent_provider_type,
                        "subagent_model": subagent_model,
                        "subagent_thinking_budget": conv.subagent_thinking_budget,
                        "subagent_api_key": subagent_api_key,
                        "subagent_endpoint_url": subagent_endpoint_url,
                        "system_prompt": conv.system_prompt_override,
                        "thinking_budget": conv.thinking_budget,
                        "tools_enabled": true,
                        "mcp_servers": mcp_configs,
                        "history": history,
                        "history_parts": history_parts,
                        "image_provider": image_provider_type,
                        "image_model": image_model,
                        "image_api_key": image_api_key,
                        "image_endpoint_url": image_endpoint_url,
                    });

                    let init_msg_text = init_msg.to_string();
                    if init_msg_text.len() > INIT_PAYLOAD_WARN_BYTES {
                        tracing::warn!(
                            conversation_id = %conversation_id,
                            payload_bytes = init_msg_text.len(),
                            "Container init payload is large; this may cause agent reconnect loops"
                        );
                    }
                    let _ = tx.try_send(init_msg_text);

                    // If there's a pending message (queued while container was starting),
                    // send it as-is (preserves deep_thinking and other fields).
                    // Otherwise fall back to re-sending the last user message from history.
                    if let Some(pending) = ws_state.take_pending_message(&conversation_id).await {
                        let _ = tx.try_send(pending);
                    } else if let Some(last) = messages.last()
                        && last.role == "user"
                    {
                        let resend = serde_json::json!({
                            "type": "user_message",
                            "message_id": &last.id,
                            "content": &last.content,
                            "deep_thinking": conv.deep_thinking,
                            "thinking_budget": conv.thinking_budget,
                            "subagent_thinking_budget": conv.subagent_thinking_budget,
                        });
                        let _ = tx.try_send(resend.to_string());
                    }
                }
            }
            ContainerMessage::Forward => {
                tracing::debug!("Forwarding to client for {}", conversation_id);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    let forwarded = with_conversation_id(&parsed, &conversation_id);
                    ws_state
                        .send_to_client(&user_id, &conversation_id, &forwarded.to_string())
                        .await;
                }
            }
            ContainerMessage::Complete {
                content,
                tool_calls,
                token_usage,
            } => {
                let content_str = content.as_deref().unwrap_or("");
                let token_count = token_usage
                    .as_ref()
                    .and_then(|u| u.get("completion"))
                    .and_then(|v| v.as_i64());
                let tool_calls_json = tool_calls
                    .as_ref()
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());

                let saved_msg = db::messages::create_message(
                    &state.db,
                    &conversation_id,
                    "assistant",
                    content_str,
                    tool_calls_json.as_deref(),
                    None,
                    token_count,
                )
                .await;

                let saved_msg = match saved_msg {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to save assistant message: {e}");
                        continue;
                    }
                };
                if let Err(e) = db::conversations::touch_conversation_activity(
                    &state.db,
                    &conversation_id,
                    &user_id,
                )
                .await
                {
                    tracing::error!(
                        conversation_id = %conversation_id,
                        error = %e,
                        "Failed to touch conversation activity after assistant completion"
                    );
                }

                // Dual-write v2 structured parts for migration.
                let parts_owned = build_parts_from_complete(content_str, tool_calls.as_ref());
                if !parts_owned.is_empty() {
                    let token_usage_json = token_usage.as_ref().map(|v| v.to_string());
                    let borrowed_parts: Vec<db::messages_v2::NewMessagePart<'_>> = parts_owned
                        .iter()
                        .map(|p| db::messages_v2::NewMessagePart {
                            part_type: &p.part_type,
                            text: p.text.as_deref(),
                            json_payload: p.json_payload.as_deref(),
                            tool_call_id: p.tool_call_id.as_deref(),
                        })
                        .collect();
                    if let Err(e) = db::messages_v2::create_message_with_parts(
                        &state.db,
                        Some(saved_msg.id.as_str()),
                        &conversation_id,
                        "assistant",
                        None,
                        None,
                        token_usage_json.as_deref(),
                        None,
                        &borrowed_parts,
                    )
                    .await
                    {
                        tracing::error!(
                            conversation_id = %conversation_id,
                            error = %e,
                            "Failed to persist assistant message to messages_v2"
                        );
                    }
                }

                let mut forwarded =
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        with_conversation_id(&parsed, &conversation_id)
                    } else {
                        serde_json::json!({
                            "type": "complete",
                            "conversation_id": conversation_id,
                        })
                    };
                if let Some(obj) = forwarded.as_object_mut() {
                    obj.insert(
                        "message_id".to_string(),
                        serde_json::Value::String(saved_msg.id),
                    );
                }
                ws_state
                    .send_to_client(&user_id, &conversation_id, &forwarded.to_string())
                    .await;
            }
            ContainerMessage::Error => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    let forwarded = with_conversation_id(&parsed, &conversation_id);
                    ws_state
                        .send_to_client(&user_id, &conversation_id, &forwarded.to_string())
                        .await;
                }
            }
        }
    }

    // Only clean up if this is still the active container for this conversation.
    // A newer container may have already replaced us (e.g. after a model switch).
    let removed = ws_state
        .remove_container_if_gen(&conversation_id, container_gen)
        .await;

    if removed {
        ws_state
            .send_to_client(
                &user_id,
                &conversation_id,
                &serde_json::json!({
                    "type": "container_status",
                    "conversation_id": conversation_id,
                    "status": "disconnected",
                    "reason": "unexpected_disconnect",
                    "message": "Container disconnected"
                })
                .to_string(),
            )
            .await;
    }

    send_task.abort();
}

async fn build_history_parts_for_init(
    pool: &sqlx::SqlitePool,
    history_messages: &[db::messages::Message],
) -> Vec<serde_json::Value> {
    let message_ids = history_messages
        .iter()
        .map(|m| m.id.clone())
        .collect::<Vec<_>>();
    let existing_v2_ids = db::messages_v2::list_existing_message_v2_ids(pool, &message_ids)
        .await
        .unwrap_or_default();
    let parts_by_message_id = db::messages_v2::list_message_parts_for_messages(pool, &message_ids)
        .await
        .unwrap_or_default();

    let mut result = Vec::with_capacity(history_messages.len());
    for m in history_messages {
        if existing_v2_ids.contains(&m.id) {
            let parts = parts_by_message_id
                .get(&m.id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|p| {
                    let payload = p
                        .json_payload
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .unwrap_or(serde_json::Value::Null);
                    serde_json::json!({
                        "type": p.part_type,
                        "text": p.text,
                        "json_payload": if payload.is_null() { serde_json::Value::Null } else { payload },
                        "tool_call_id": p.tool_call_id,
                        "seq": p.seq,
                    })
                })
                .collect::<Vec<_>>();
            result.push(serde_json::json!({
                "role": m.role,
                "parts": parts,
            }));
            continue;
        }

        result.push(serde_json::json!({
            "role": m.role,
            "parts": legacy_parts_for_init(m),
        }));
    }
    result
}

fn legacy_parts_for_init(m: &db::messages::Message) -> Vec<serde_json::Value> {
    db::messages_v2::legacy_message_to_parts(m)
        .into_iter()
        .enumerate()
        .map(|(idx, p)| {
            let payload = p
                .json_payload
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "type": p.part_type,
                "text": p.text,
                "json_payload": if payload.is_null() { serde_json::Value::Null } else { payload },
                "tool_call_id": p.tool_call_id,
                "seq": idx,
            })
        })
        .collect()
}

fn build_parts_from_complete(
    content: &str,
    tool_calls: Option<&serde_json::Value>,
) -> Vec<db::messages_v2::NewMessagePartOwned> {
    db::messages_v2::content_blocks_to_parts(content, tool_calls)
}

fn with_conversation_id(parsed: &serde_json::Value, conversation_id: &str) -> serde_json::Value {
    let mut forwarded = parsed.clone();
    if let Some(obj) = forwarded.as_object_mut() {
        obj.insert(
            "conversation_id".to_string(),
            serde_json::Value::String(conversation_id.to_string()),
        );
    }
    forwarded
}

#[cfg(test)]
mod tests {
    use super::{
        build_parts_from_complete, legacy_parts_for_init, resolve_conversation_providers,
        with_conversation_id,
    };
    use crate::db::{conversations::Conversation, messages::Message, providers::UserProvider};
    use tokio::sync::mpsc;

    fn mk_provider(
        id: &str,
        provider: &str,
        models: &[&str],
        image_models: &[&str],
    ) -> UserProvider {
        UserProvider {
            id: id.to_string(),
            user_id: "u1".to_string(),
            provider: provider.to_string(),
            api_key_encrypted: "encrypted".to_string(),
            endpoint_url: None,
            model_name: models.first().map(|m| (*m).to_string()),
            is_default: false,
            created_at: "now".to_string(),
            models: Some(serde_json::to_string(models).unwrap()),
            name: Some(id.to_string()),
            image_models: Some(serde_json::to_string(image_models).unwrap()),
        }
    }

    fn mk_conversation() -> Conversation {
        Conversation {
            id: "c1".to_string(),
            user_id: "u1".to_string(),
            title: "t".to_string(),
            provider_id: Some("chat".to_string()),
            model_name: Some("gpt-4o".to_string()),
            subagent_provider_id: Some("sub".to_string()),
            subagent_model: Some("gpt-4.1-mini".to_string()),
            system_prompt_override: None,
            deep_thinking: true,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            image_provider_id: None,
            image_model: None,
            share_token: None,
            thinking_budget: Some(128000),
            subagent_thinking_budget: Some(128000),
        }
    }

    #[test]
    fn resolve_conversation_providers_requires_explicit_models() {
        let mut conv = mk_conversation();
        conv.subagent_provider_id = None;
        let providers = vec![
            mk_provider("chat", "openai", &["gpt-4o"], &[]),
            mk_provider("sub", "openai", &["gpt-4.1-mini"], &[]),
        ];
        let err = resolve_conversation_providers(&conv, &providers).unwrap_err();
        assert!(err.contains("subagent_provider_id"));
    }

    #[test]
    fn resolve_conversation_providers_rejects_model_mismatch() {
        let conv = mk_conversation();
        let providers = vec![
            mk_provider("chat", "openai", &["gpt-4o"], &[]),
            mk_provider("sub", "openai", &["gpt-4o"], &[]),
        ];
        let err = resolve_conversation_providers(&conv, &providers).unwrap_err();
        assert!(err.contains("gpt-4.1-mini"));
    }

    #[test]
    fn resolve_conversation_providers_allows_optional_image_disabled() {
        let conv = mk_conversation();
        let providers = vec![
            mk_provider("chat", "openai", &["gpt-4o"], &[]),
            mk_provider("sub", "openai", &["gpt-4.1-mini"], &[]),
        ];
        let resolved = resolve_conversation_providers(&conv, &providers).unwrap();
        assert!(resolved.image_provider.is_none());
        assert!(resolved.image_model.is_none());
    }

    #[test]
    fn build_parts_from_complete_uses_structured_blocks() {
        let payload = serde_json::json!([
            {"type":"thinking","content":"reasoning..."},
            {"type":"text","content":"hello"},
            {"type":"tool_call","id":"tc-1","name":"bash","input":{"command":"ls"}}
        ]);
        let parts = build_parts_from_complete("ignored", Some(&payload));
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].part_type, "reasoning");
        assert_eq!(parts[0].text.as_deref(), Some("reasoning..."));
        assert_eq!(parts[1].part_type, "text");
        assert_eq!(parts[1].text.as_deref(), Some("hello"));
        assert_eq!(parts[2].part_type, "tool_call");
        assert_eq!(parts[2].tool_call_id.as_deref(), Some("tc-1"));
        assert!(parts[2].json_payload.as_deref().is_some());
    }

    #[test]
    fn build_parts_from_complete_extracts_tool_result() {
        let payload = serde_json::json!([
            {
                "type":"tool_call",
                "id":"tc-2",
                "name":"bash",
                "input":{"command":"pwd"},
                "result":{"kind":"bash","text":"/workspace"}
            }
        ]);
        let parts = build_parts_from_complete("ignored", Some(&payload));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].part_type, "tool_call");
        assert_eq!(parts[1].part_type, "tool_result");
        assert_eq!(parts[1].tool_call_id.as_deref(), Some("tc-2"));
        assert_eq!(parts[1].text.as_deref(), Some("/workspace"));
    }

    #[test]
    fn build_parts_from_complete_falls_back_to_plain_text() {
        let parts = build_parts_from_complete("plain answer", None);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_type, "text");
        assert_eq!(parts[0].text.as_deref(), Some("plain answer"));
    }

    #[test]
    fn legacy_parts_for_init_maps_tool_blocks() {
        let msg = Message {
            id: "m1".to_string(),
            conversation_id: "c1".to_string(),
            role: "assistant".to_string(),
            content: "ignored".to_string(),
            tool_calls: Some(
                r#"[{"type":"thinking","content":"plan"},{"type":"text","content":"answer"},{"type":"tool_call","id":"tc1","name":"bash","input":{"command":"ls"}}]"#.to_string()
            ),
            tool_call_id: None,
            token_count: None,
            created_at: "now".to_string(),
        };
        let parts = legacy_parts_for_init(&msg);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["type"], "reasoning");
        assert_eq!(parts[1]["type"], "text");
        assert_eq!(parts[2]["type"], "tool_call");
        assert_eq!(parts[2]["tool_call_id"], "tc1");
    }

    #[test]
    fn legacy_parts_for_init_includes_tool_result() {
        let msg = Message {
            id: "m2".to_string(),
            conversation_id: "c1".to_string(),
            role: "assistant".to_string(),
            content: "ignored".to_string(),
            tool_calls: Some(
                r#"[{"type":"tool_call","id":"tc1","name":"bash","input":{"command":"ls"},"result":{"kind":"bash","text":"a\nb"}}]"#
                    .to_string(),
            ),
            tool_call_id: None,
            token_count: None,
            created_at: "now".to_string(),
        };
        let parts = legacy_parts_for_init(&msg);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[1]["type"], "tool_result");
        assert_eq!(parts[1]["tool_call_id"], "tc1");
        assert_eq!(parts[1]["text"], "a\nb");
    }

    #[test]
    fn with_conversation_id_preserves_task_trace_delta_payload() {
        let event = serde_json::json!({
            "type": "task_trace_delta",
            "tool_call_id": "tc-task-1",
            "event_type": "tool_result",
            "payload": {
                "tool_call_id": "sub-tc-1",
                "result": {
                    "kind": "read",
                    "text": "ok",
                    "success": true,
                    "error": null,
                    "data": {"trace": [{"type":"text","content":"Investigating"}]},
                    "meta": {}
                },
                "is_error": false
            }
        });

        let forwarded = with_conversation_id(&event, "conv-123");

        assert_eq!(forwarded["type"], "task_trace_delta");
        assert_eq!(forwarded["tool_call_id"], "tc-task-1");
        assert_eq!(forwarded["event_type"], "tool_result");
        assert_eq!(forwarded["payload"]["tool_call_id"], "sub-tc-1");
        assert_eq!(forwarded["payload"]["result"]["kind"], "read");
        assert_eq!(
            forwarded["payload"]["result"]["data"]["trace"][0]["content"],
            "Investigating"
        );
        assert_eq!(forwarded["conversation_id"], "conv-123");
    }

    #[tokio::test]
    async fn forward_task_trace_delta_roundtrip_preserves_payload_over_ws_state() {
        let ws_state = crate::ws::WsState::new();
        let (tx, mut rx) = mpsc::channel(crate::ws::WS_CHANNEL_CAPACITY);
        ws_state.add_client("user-1", "conv-123", tx).await;

        let event = serde_json::json!({
            "type": "task_trace_delta",
            "tool_call_id": "tc-task-1",
            "event_type": "tool_result",
            "payload": {
                "tool_call_id": "sub-tc-1",
                "result": {
                    "kind": "read",
                    "text": "ok",
                    "success": true,
                    "error": null,
                    "data": {"trace": [{"type":"text","content":"Investigating"}]},
                    "meta": {}
                },
                "is_error": false
            }
        });
        let forwarded = with_conversation_id(&event, "conv-123");

        ws_state
            .send_to_client("user-1", "conv-123", &forwarded.to_string())
            .await;

        let raw = rx.recv().await.expect("forwarded message");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");

        assert_eq!(parsed["type"], "task_trace_delta");
        assert_eq!(parsed["tool_call_id"], "tc-task-1");
        assert_eq!(parsed["event_type"], "tool_result");
        assert_eq!(parsed["payload"]["tool_call_id"], "sub-tc-1");
        assert_eq!(parsed["payload"]["result"]["kind"], "read");
        assert_eq!(
            parsed["payload"]["result"]["data"]["trace"][0]["content"],
            "Investigating"
        );
        assert_eq!(parsed["conversation_id"], "conv-123");
    }

    #[test]
    fn with_conversation_id_preserves_subagent_trace_delta_payload() {
        let event = serde_json::json!({
            "type": "subagent_trace_delta",
            "tool_call_id": "tc-explore-1",
            "event_type": "tool_result",
            "payload": {
                "tool_call_id": "sub-tc-2",
                "result": {
                    "kind": "read",
                    "text": "ok",
                    "success": true,
                    "error": null,
                    "data": {"trace": [{"type":"text","content":"Investigating"}]},
                    "meta": {}
                },
                "is_error": false
            }
        });

        let forwarded = with_conversation_id(&event, "conv-123");

        assert_eq!(forwarded["type"], "subagent_trace_delta");
        assert_eq!(forwarded["tool_call_id"], "tc-explore-1");
        assert_eq!(forwarded["event_type"], "tool_result");
        assert_eq!(forwarded["payload"]["tool_call_id"], "sub-tc-2");
        assert_eq!(forwarded["payload"]["result"]["kind"], "read");
        assert_eq!(
            forwarded["payload"]["result"]["data"]["trace"][0]["content"],
            "Investigating"
        );
        assert_eq!(forwarded["conversation_id"], "conv-123");
    }

    #[tokio::test]
    async fn forward_subagent_trace_delta_roundtrip_preserves_payload_over_ws_state() {
        let ws_state = crate::ws::WsState::new();
        let (tx, mut rx) = mpsc::channel(crate::ws::WS_CHANNEL_CAPACITY);
        ws_state.add_client("user-1", "conv-123", tx).await;

        let event = serde_json::json!({
            "type": "subagent_trace_delta",
            "tool_call_id": "tc-explore-1",
            "event_type": "tool_result",
            "payload": {
                "tool_call_id": "sub-tc-2",
                "result": {
                    "kind": "read",
                    "text": "ok",
                    "success": true,
                    "error": null,
                    "data": {"trace": [{"type":"text","content":"Investigating"}]},
                    "meta": {}
                },
                "is_error": false
            }
        });
        let forwarded = with_conversation_id(&event, "conv-123");

        ws_state
            .send_to_client("user-1", "conv-123", &forwarded.to_string())
            .await;

        let raw = rx.recv().await.expect("forwarded message");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");

        assert_eq!(parsed["type"], "subagent_trace_delta");
        assert_eq!(parsed["tool_call_id"], "tc-explore-1");
        assert_eq!(parsed["event_type"], "tool_result");
        assert_eq!(parsed["payload"]["tool_call_id"], "sub-tc-2");
        assert_eq!(parsed["payload"]["result"]["kind"], "read");
        assert_eq!(
            parsed["payload"]["result"]["data"]["trace"][0]["content"],
            "Investigating"
        );
        assert_eq!(parsed["conversation_id"], "conv-123");
    }
}
