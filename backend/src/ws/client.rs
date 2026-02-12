use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::WsState;
use super::messages::ClientMessage;
use crate::auth;
use crate::auth::middleware::AppState;
use crate::db;
use crate::docker::manager::DockerManager;

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

async fn send_to_container_or_start(
    ws_state: &Arc<WsState>,
    docker_manager: &Arc<DockerManager>,
    tx: &mpsc::UnboundedSender<String>,
    conv_id: &str,
    user_id: &str,
    message: &str,
) {
    let sent = ws_state.send_to_container(conv_id, message).await;
    if !sent {
        // Queue the message so the container handler can forward it (with all fields) on ready
        ws_state.set_pending_message(conv_id, message.to_string()).await;

        let _ = tx.send(
            serde_json::json!({
                "type": "container_status",
                "conversation_id": conv_id,
                "status": "starting",
                "message": "Container not connected. Starting..."
            })
            .to_string(),
        );

        let dm = docker_manager.clone();
        let cid = conv_id.to_string();
        let uid = user_id.to_string();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            match dm.start_container(&cid, &uid).await {
                Ok(container_id) => {
                    tracing::info!("Container {container_id} started for {cid}");
                }
                Err(e) => {
                    tracing::error!("Failed to start container for {cid}: {e}");
                    let _ = tx2.send(
                        serde_json::json!({
                            "type": "error",
                            "code": "container_start_failed",
                            "message": format!("Failed to start container: {e}")
                        })
                        .to_string(),
                    );
                }
            }
        });
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let claims = match auth::verify_access_token(&query.token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

    let ws_state = state.ws_state.clone();
    let docker_manager = state.docker_manager.clone();

    ws.on_upgrade(move |socket| handle_client_ws(socket, claims.sub, state, ws_state, docker_manager))
}

async fn handle_client_ws(
    socket: WebSocket,
    user_id: String,
    state: Arc<AppState>,
    ws_state: Arc<WsState>,
    docker_manager: Arc<DockerManager>,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    let mut current_conversation_id: Option<String> = None;

    while let Some(Ok(msg)) = ws_stream.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match client_msg {
            ClientMessage::JoinConversation { conversation_id: conv_id } => {
                if conv_id.is_empty() {
                    continue;
                }

                match db::conversations::get_conversation(&state.db, &conv_id, &user_id).await {
                    Ok(None) | Err(_) => {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "not_found",
                                "message": "Conversation not found"
                            })
                            .to_string(),
                        );
                        continue;
                    }
                    Ok(Some(_)) => {}
                }

                if let Some(ref old_id) = current_conversation_id {
                    ws_state.remove_client(&user_id, old_id).await;
                }

                current_conversation_id = Some(conv_id.to_string());
                ws_state.add_client(&user_id, &conv_id, tx.clone()).await;

                let _ = tx.send(
                    serde_json::json!({
                        "type": "conversation_joined",
                        "conversation_id": conv_id,
                    })
                    .to_string(),
                );
            }
            ClientMessage::UserMessage { content } => {
                let conv_id = match &current_conversation_id {
                    Some(id) => id.clone(),
                    None => {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "no_conversation",
                                "message": "Join a conversation first"
                            })
                            .to_string(),
                        );
                        continue;
                    }
                };

                if content.is_empty() {
                    continue;
                }

                let msg = match db::messages::create_message(&state.db, &conv_id, "user", &content, None, None, None).await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to create message: {e}");
                        continue;
                    }
                };

                let conv = db::conversations::get_conversation(&state.db, &conv_id, &user_id).await.ok().flatten();
                let deep_thinking = conv.as_ref().map(|c| c.deep_thinking).unwrap_or(false);

                // Auto-generate conversation title from first message
                let msg_count = db::messages::count_messages(&state.db, &conv_id).await.unwrap_or(0);
                if msg_count == 1 {
                    let title: String = if content.chars().count() > 50 {
                        format!("{}...", content.chars().take(50).collect::<String>())
                    } else {
                        content.clone()
                    };
                    if let Some(c) = &conv {
                        let _ = db::conversations::update_conversation(
                            &state.db, &conv_id, &user_id, &title,
                            c.provider.as_deref(), c.model_name.as_deref(),
                            c.system_prompt_override.as_deref(), c.deep_thinking,
                        )
                        .await;
                    }
                }

                let _ = tx.send(
                    serde_json::json!({
                        "type": "message_saved",
                        "conversation_id": conv_id,
                        "message_id": msg.id,
                    })
                    .to_string(),
                );

                tracing::debug!("user_message: deep_thinking={}", deep_thinking);

                send_to_container_or_start(
                    &ws_state, &docker_manager, &tx, &conv_id, &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": msg.id,
                        "content": content,
                        "deep_thinking": deep_thinking,
                    })
                    .to_string(),
                )
                .await;
            }
            ClientMessage::EditMessage { message_id, content } => {
                let conv_id = match &current_conversation_id {
                    Some(id) => id.clone(),
                    None => continue,
                };

                if content.is_empty() {
                    continue;
                }

                // Validate message exists and is a user message
                let msg = match db::messages::get_message(&state.db, &message_id).await {
                    Ok(Some(m)) if m.role == "user" && m.conversation_id == conv_id => m,
                    _ => {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "invalid_message",
                                "message": "Message not found or not a user message"
                            })
                            .to_string(),
                        );
                        continue;
                    }
                };

                // Update content and delete subsequent messages
                let all_msgs = db::messages::list_messages(&state.db, &conv_id, 1000, 0).await.unwrap_or_default();
                let keep_turns = all_msgs.iter()
                    .take_while(|m| m.id != msg.id)
                    .filter(|m| m.role == "user")
                    .count();

                db::messages::update_message_content(&state.db, &msg.id, &content).await.ok();
                db::messages::delete_messages_after(&state.db, &conv_id, &msg.id).await.ok();

                let _ = tx.send(
                    serde_json::json!({
                        "type": "messages_truncated",
                        "after_message_id": msg.id,
                        "updated_content": content,
                    })
                    .to_string(),
                );

                let deep_thinking = db::conversations::get_conversation(&state.db, &conv_id, &user_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| c.deep_thinking)
                    .unwrap_or(false);

                // Tell the running container to truncate its in-memory history
                ws_state.send_to_container(&conv_id, &serde_json::json!({
                    "type": "truncate_history",
                    "keep_turns": keep_turns,
                }).to_string()).await;

                send_to_container_or_start(
                    &ws_state, &docker_manager, &tx, &conv_id, &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": msg.id,
                        "content": content,
                        "deep_thinking": deep_thinking,
                    })
                    .to_string(),
                )
                .await;
            }
            ClientMessage::Regenerate { message_id } => {
                let conv_id = match &current_conversation_id {
                    Some(id) => id.clone(),
                    None => continue,
                };

                // Validate message exists and is an assistant message
                let msg = match db::messages::get_message(&state.db, &message_id).await {
                    Ok(Some(m)) if m.role == "assistant" && m.conversation_id == conv_id => m,
                    _ => {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "invalid_message",
                                "message": "Message not found or not an assistant message"
                            })
                            .to_string(),
                        );
                        continue;
                    }
                };

                // Find the last user message before this assistant message
                let all_msgs = db::messages::list_messages(&state.db, &conv_id, 1000, 0).await.unwrap_or_default();
                let msg_idx = all_msgs.iter().position(|m| m.id == msg.id);
                let last_user_msg = msg_idx.and_then(|idx| {
                    all_msgs[..idx].iter().rev().find(|m| m.role == "user")
                });

                let user_msg = match last_user_msg {
                    Some(m) => m.clone(),
                    None => continue,
                };

                // Delete the assistant message and everything after the user message
                let keep_turns = all_msgs.iter()
                    .take_while(|m| m.id != user_msg.id)
                    .filter(|m| m.role == "user")
                    .count();

                db::messages::delete_messages_after(&state.db, &conv_id, &user_msg.id).await.ok();

                let _ = tx.send(
                    serde_json::json!({
                        "type": "messages_truncated",
                        "after_message_id": user_msg.id,
                    })
                    .to_string(),
                );

                let deep_thinking = db::conversations::get_conversation(&state.db, &conv_id, &user_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| c.deep_thinking)
                    .unwrap_or(false);

                // Tell the running container to truncate its in-memory history
                ws_state.send_to_container(&conv_id, &serde_json::json!({
                    "type": "truncate_history",
                    "keep_turns": keep_turns,
                }).to_string()).await;

                send_to_container_or_start(
                    &ws_state, &docker_manager, &tx, &conv_id, &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": user_msg.id,
                        "content": user_msg.content,
                        "deep_thinking": deep_thinking,
                    })
                    .to_string(),
                )
                .await;
            }
            ClientMessage::Cancel => {
                if let Some(ref conv_id) = current_conversation_id {
                    ws_state
                        .send_to_container(
                            conv_id,
                            &serde_json::json!({"type": "cancel"}).to_string(),
                        )
                        .await;
                }
            }
            ClientMessage::Ping => {
                let _ = tx.send(serde_json::json!({"type": "pong"}).to_string());
            }
        }
    }

    if let Some(ref conv_id) = current_conversation_id {
        ws_state.remove_client(&user_id, conv_id).await;
    }
    send_task.abort();
}
