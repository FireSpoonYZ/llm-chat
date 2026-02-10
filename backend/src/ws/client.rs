use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, Query, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::WsState;
use crate::auth;
use crate::auth::middleware::AppState;
use crate::db;
use crate::docker::manager::DockerManager;

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    Extension(state): Extension<Arc<AppState>>,
    Extension(ws_state): Extension<Arc<WsState>>,
    Extension(docker_manager): Extension<Arc<DockerManager>>,
) -> impl IntoResponse {
    let claims = match auth::verify_access_token(&query.token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

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

        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        match msg_type {
            "join_conversation" => {
                let conv_id = parsed
                    .get("conversation_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if conv_id.is_empty() {
                    continue;
                }

                if db::conversations::get_conversation(&state.db, conv_id, &user_id)
                    .await
                    .is_none()
                {
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

                if let Some(ref old_id) = current_conversation_id {
                    ws_state.remove_client(&user_id, old_id).await;
                }

                current_conversation_id = Some(conv_id.to_string());
                ws_state.add_client(&user_id, conv_id, tx.clone()).await;

                let _ = tx.send(
                    serde_json::json!({
                        "type": "conversation_joined",
                        "conversation_id": conv_id,
                    })
                    .to_string(),
                );
            }
            "user_message" => {
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

                let content = parsed
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if content.is_empty() {
                    continue;
                }

                let msg =
                    db::messages::create_message(&state.db, &conv_id, "user", &content, None, None, None)
                        .await;

                // Auto-generate conversation title from first message
                let msg_count = db::messages::count_messages(&state.db, &conv_id).await;
                if msg_count == 1 {
                    let title = if content.len() > 50 {
                        format!("{}...", &content[..50])
                    } else {
                        content.clone()
                    };
                    db::conversations::update_conversation(
                        &state.db, &conv_id, &user_id, &title, None, None, None,
                    )
                    .await;
                }

                let _ = tx.send(
                    serde_json::json!({
                        "type": "message_saved",
                        "conversation_id": conv_id,
                        "message_id": msg.id,
                    })
                    .to_string(),
                );

                let sent = ws_state
                    .send_to_container(
                        &conv_id,
                        &serde_json::json!({
                            "type": "user_message",
                            "message_id": msg.id,
                            "content": content,
                        })
                        .to_string(),
                    )
                    .await;

                if !sent {
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
                    let cid = conv_id.clone();
                    let uid = user_id.clone();
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
            "cancel" => {
                if let Some(ref conv_id) = current_conversation_id {
                    ws_state
                        .send_to_container(
                            conv_id,
                            &serde_json::json!({"type": "cancel"}).to_string(),
                        )
                        .await;
                }
            }
            _ => {}
        }
    }

    if let Some(ref conv_id) = current_conversation_id {
        ws_state.remove_client(&user_id, conv_id).await;
    }
    send_task.abort();
}
