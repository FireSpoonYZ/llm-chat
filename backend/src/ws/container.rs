use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, Query, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::WsState;
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
    Extension(state): Extension<Arc<AppState>>,
    Extension(ws_state): Extension<Arc<WsState>>,
) -> impl IntoResponse {
    let claims = match auth::verify_container_token(&query.token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

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

    ws_state.add_container(&conversation_id, tx.clone()).await;

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

        let msg_type = parsed
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        tracing::debug!("Container msg for {}: type={}", conversation_id, msg_type);

        match msg_type {
            "ready" => {
                tracing::info!("Container ready for conversation {}", conversation_id);
                if let Some(conv) =
                    db::conversations::get_conversation(&state.db, &conversation_id, &user_id)
                        .await
                {
                    let provider_name = conv.provider.as_deref().unwrap_or("");
                    let provider = if provider_name.is_empty() {
                        // No provider on conversation, use user's default
                        db::providers::get_default_provider(&state.db, &user_id).await
                    } else {
                        db::providers::get_provider(&state.db, &user_id, provider_name).await
                    };
                    let provider_name = provider.as_ref()
                        .map(|p| p.provider.as_str())
                        .unwrap_or("openai");

                    let messages =
                        db::messages::list_messages(&state.db, &conversation_id, 50, 0).await;

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
                            .await;
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

                    let init_msg = serde_json::json!({
                        "type": "init",
                        "conversation_id": conversation_id,
                        "provider": provider_name,
                        "model": model,
                        "api_key": api_key,
                        "endpoint_url": provider.as_ref().and_then(|p| p.endpoint_url.clone()),
                        "system_prompt": conv.system_prompt_override,
                        "tools_enabled": true,
                        "mcp_servers": mcp_configs,
                        "history": history,
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
            "assistant_delta" | "thinking_delta" | "tool_call" | "tool_result" => {
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
            "complete" => {
                let content = parsed
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let token_count = parsed
                    .get("token_usage")
                    .and_then(|u| u.get("completion"))
                    .and_then(|v| v.as_i64());
                let tool_calls_json = parsed
                    .get("tool_calls")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());

                let saved_msg = db::messages::create_message(
                    &state.db,
                    &conversation_id,
                    "assistant",
                    content,
                    tool_calls_json.as_deref(),
                    None,
                    token_count,
                )
                .await;

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
            "error" => {
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
            _ => {
                tracing::debug!("Unhandled container msg type: {}", msg_type);
            }
        }
    }

    ws_state.remove_container(&conversation_id).await;
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

    send_task.abort();
}
