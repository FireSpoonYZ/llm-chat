use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
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
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

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

        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let container_msg: ContainerMessage = match serde_json::from_value(parsed.clone()) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let msg_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
        tracing::debug!("Container msg for {}: type={}", conversation_id, msg_type);

        match container_msg {
            ContainerMessage::Ready => {
                tracing::info!("Container ready for conversation {}", conversation_id);
                if let Some(conv) =
                    db::conversations::get_conversation(&state.db, &conversation_id, &user_id)
                        .await
                        .ok()
                        .flatten()
                {
                    // Look up chat provider by name (not type)
                    let chat_provider_name = conv.provider.as_deref().unwrap_or("");
                    let provider = if chat_provider_name.is_empty() {
                        db::providers::get_default_provider(&state.db, &user_id).await.ok().flatten()
                    } else {
                        db::providers::get_provider_by_name(&state.db, &user_id, chat_provider_name).await.ok().flatten()
                    };
                    let provider_type = provider.as_ref()
                        .map(|p| p.provider.as_str())
                        .unwrap_or("openai");

                    // Look up image provider (separate from chat provider)
                    let image_provider_name = conv.image_provider.as_deref().unwrap_or("");
                    let image_provider = if image_provider_name.is_empty() {
                        None
                    } else {
                        db::providers::get_provider_by_name(&state.db, &user_id, image_provider_name).await.ok().flatten()
                    };

                    let messages =
                        db::messages::list_messages(&state.db, &conversation_id, super::CONTAINER_INIT_HISTORY_LIMIT, 0).await.unwrap_or_default();

                    // Check if last message is from user — it will be resent separately
                    let needs_resend = messages.last().map_or(false, |m| m.role == "user");
                    let history_messages = if needs_resend {
                        &messages[..messages.len() - 1]
                    } else {
                        &messages[..]
                    };

                    let history: Vec<serde_json::Value> = history_messages
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "role": m.role,
                                "content": m.content,
                            })
                        })
                        .collect();

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
                            })
                        })
                        .collect();

                    let api_key = provider
                        .as_ref()
                        .map(|p| {
                            crate::crypto::decrypt(
                                &p.api_key_encrypted,
                                &state.config.encryption_key,
                            )
                            .unwrap_or_default()
                        })
                        .unwrap_or_default();

                    let first_model_from_provider = provider.as_ref().and_then(|p| {
                        p.models.as_deref()
                            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                            .and_then(|v| v.into_iter().next())
                            .or_else(|| p.model_name.clone())
                    });
                    let model = conv.model_name
                        .or(first_model_from_provider)
                        .unwrap_or_else(|| "gpt-4o".to_string());

                    // Decrypt image provider API key and resolve type
                    let image_api_key = image_provider
                        .as_ref()
                        .map(|p| {
                            crate::crypto::decrypt(
                                &p.api_key_encrypted,
                                &state.config.encryption_key,
                            )
                            .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    let image_provider_type = image_provider.as_ref()
                        .map(|p| p.provider.as_str())
                        .unwrap_or("");
                    let image_endpoint_url = image_provider.as_ref()
                        .and_then(|p| p.endpoint_url.clone());

                    let init_msg = serde_json::json!({
                        "type": "init",
                        "conversation_id": conversation_id,
                        "provider": provider_type,
                        "model": model,
                        "api_key": api_key,
                        "endpoint_url": provider.as_ref().and_then(|p| p.endpoint_url.clone()),
                        "system_prompt": conv.system_prompt_override,
                        "tools_enabled": true,
                        "mcp_servers": mcp_configs,
                        "history": history,
                        "image_provider": image_provider_type,
                        "image_model": conv.image_model,
                        "image_api_key": image_api_key,
                        "image_endpoint_url": image_endpoint_url,
                    });

                    let _ = tx.send(init_msg.to_string());

                    // If there's a pending message (queued while container was starting),
                    // send it as-is (preserves deep_thinking and other fields).
                    // Otherwise fall back to re-sending the last user message from history.
                    if let Some(pending) = ws_state.take_pending_message(&conversation_id).await {
                        let _ = tx.send(pending);
                    } else if let Some(last) = messages.last() {
                        if last.role == "user" {
                            let resend = serde_json::json!({
                                "type": "user_message",
                                "message_id": &last.id,
                                "content": &last.content,
                                "deep_thinking": conv.deep_thinking,
                            });
                            let _ = tx.send(resend.to_string());
                        }
                    }
                }
            }
            ContainerMessage::Forward => {
                tracing::debug!("Forwarding {} to client for {}", msg_type, conversation_id);
                let mut forwarded = parsed.clone();
                if let Some(obj) = forwarded.as_object_mut() {
                    obj.insert(
                        "conversation_id".to_string(),
                        serde_json::Value::String(conversation_id.clone()),
                    );
                }
                ws_state
                    .send_to_client(&user_id, &conversation_id, &forwarded.to_string())
                    .await;
            }
            ContainerMessage::Complete { content, tool_calls, token_usage } => {
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

                let mut forwarded = parsed.clone();
                if let Some(obj) = forwarded.as_object_mut() {
                    obj.insert(
                        "conversation_id".to_string(),
                        serde_json::Value::String(conversation_id.clone()),
                    );
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
                let mut forwarded = parsed.clone();
                if let Some(obj) = forwarded.as_object_mut() {
                    obj.insert(
                        "conversation_id".to_string(),
                        serde_json::Value::String(conversation_id.clone()),
                    );
                }
                ws_state
                    .send_to_client(&user_id, &conversation_id, &forwarded.to_string())
                    .await;
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
                    "message": "Container disconnected"
                })
                .to_string(),
            )
            .await;
    }

    send_task.abort();
}
