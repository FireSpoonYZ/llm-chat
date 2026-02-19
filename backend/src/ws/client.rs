use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, header},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::WsState;
use super::messages::ClientMessage;
use crate::auth;
use crate::auth::middleware::AppState;
use crate::db;
use crate::docker::manager::DockerManager;

fn ws_parts_from_db_parts(parts: &[db::messages_v2::MessagePart]) -> Vec<serde_json::Value> {
    parts
        .iter()
        .map(|part| {
            let json_payload = part
                .json_payload
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
            serde_json::json!({
                "type": part.part_type,
                "text": part.text,
                "json_payload": json_payload,
                "tool_call_id": part.tool_call_id,
                "seq": part.seq,
            })
        })
        .collect()
}

fn extract_ws_access_token(headers: &HeaderMap) -> Option<String> {
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok())
        && let Some(token) = auth_header.strip_prefix("Bearer ")
    {
        return Some(token.to_owned());
    }
    auth::get_cookie(headers, auth::ACCESS_COOKIE_NAME)
}

fn ws_origin_allowed(headers: &HeaderMap, allowed_origins: &Option<String>) -> bool {
    let Some(configured) = allowed_origins.as_deref() else {
        return true;
    };

    let configured: Vec<&str> = configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if configured.is_empty() {
        return true;
    }

    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        // Non-browser WS clients may omit Origin.
        return true;
    };

    configured.contains(&origin)
}

fn should_touch_after_edit(
    message_updated: bool,
    deleted_messages: bool,
    deleted_messages_v2: bool,
) -> bool {
    message_updated && deleted_messages && deleted_messages_v2
}

fn should_touch_after_regenerate(deleted_messages: bool, deleted_messages_v2: bool) -> bool {
    deleted_messages && deleted_messages_v2
}

fn validate_question_answer_payload(
    questionnaire_id: &str,
    answers: &serde_json::Value,
) -> Result<(), &'static str> {
    if questionnaire_id.trim().is_empty() {
        return Err("Missing questionnaire_id");
    }
    if !answers.is_array() {
        return Err("answers must be an array");
    }
    Ok(())
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
    if sent {
        // Refresh activity so the idle timeout doesn't kill the container
        // between user send and container response.
        docker_manager.touch_activity(conv_id).await;
    } else {
        // Queue the message so the container handler can forward it (with all fields) on ready
        ws_state
            .set_pending_message(conv_id, message.to_string())
            .await;

        let _ = tx.send(
            serde_json::json!({
                "type": "container_status",
                "conversation_id": conv_id,
                "status": "starting",
                "reason": "auto_restart",
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
                            "message": "Failed to start container. Please try again later."
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
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !ws_origin_allowed(&headers, &state.config.cors_allowed_origins) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let token = match extract_ws_access_token(&headers) {
        Some(token) => token,
        None => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

    let claims = match auth::verify_access_token(&token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

    let ws_state = state.ws_state.clone();
    let docker_manager = state.docker_manager.clone();

    ws.on_upgrade(move |socket| {
        handle_client_ws(socket, claims.sub, state, ws_state, docker_manager)
    })
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
            ClientMessage::JoinConversation {
                conversation_id: conv_id,
            } => {
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
            ClientMessage::UserMessage {
                content,
                attachments,
            } => {
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

                let msg = match db::messages::create_message(
                    &state.db, &conv_id, "user", &content, None, None, None,
                )
                .await
                {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to create message: {e}");
                        continue;
                    }
                };
                if let Err(e) = db::messages_v2::upsert_message_text_part(
                    &state.db, &msg.id, &conv_id, "user", &content,
                )
                .await
                {
                    tracing::error!(
                        conversation_id = %conv_id,
                        message_id = %msg.id,
                        error = %e,
                        "Failed to persist user message to messages_v2"
                    );
                }
                if let Err(e) =
                    db::conversations::touch_conversation_activity(&state.db, &conv_id, &user_id)
                        .await
                {
                    tracing::error!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to touch conversation activity after user message"
                    );
                }

                let conv = db::conversations::get_conversation(&state.db, &conv_id, &user_id)
                    .await
                    .ok()
                    .flatten();
                let deep_thinking = conv.as_ref().map(|c| c.deep_thinking).unwrap_or(false);
                let thinking_budget = conv.as_ref().and_then(|c| c.thinking_budget);
                let subagent_thinking_budget =
                    conv.as_ref().and_then(|c| c.subagent_thinking_budget);

                // Auto-generate conversation title from first message
                let msg_count = db::messages::count_messages(&state.db, &conv_id)
                    .await
                    .unwrap_or(0);
                if msg_count == 1 {
                    let title: String = if content.chars().count() > 50 {
                        format!("{}...", content.chars().take(50).collect::<String>())
                    } else {
                        content.clone()
                    };
                    if let Some(c) = &conv {
                        let _ = db::conversations::update_conversation_with_subagent(
                            &state.db,
                            &conv_id,
                            &user_id,
                            &title,
                            c.provider.as_deref(),
                            c.model_name.as_deref(),
                            c.subagent_provider.as_deref(),
                            c.subagent_model.as_deref(),
                            c.system_prompt_override.as_deref(),
                            c.deep_thinking,
                            c.image_provider.as_deref(),
                            c.image_model.as_deref(),
                            c.thinking_budget,
                            c.subagent_thinking_budget,
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
                    &ws_state,
                    &docker_manager,
                    &tx,
                    &conv_id,
                    &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": msg.id,
                        "content": content,
                        "deep_thinking": deep_thinking,
                        "thinking_budget": thinking_budget,
                        "subagent_thinking_budget": subagent_thinking_budget,
                        "attachments": attachments,
                    })
                    .to_string(),
                )
                .await;
            }
            ClientMessage::QuestionAnswer {
                questionnaire_id,
                answers,
            } => {
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

                if let Err(message) = validate_question_answer_payload(&questionnaire_id, &answers)
                {
                    let _ = tx.send(
                        serde_json::json!({
                            "type": "error",
                            "code": "invalid_question_answer",
                            "message": message
                        })
                        .to_string(),
                    );
                    continue;
                }

                let forwarded = serde_json::json!({
                    "type": "question_answer",
                    "questionnaire_id": questionnaire_id,
                    "answers": answers,
                })
                .to_string();

                let sent = ws_state.send_to_container(&conv_id, &forwarded).await;
                if sent {
                    docker_manager.touch_activity(&conv_id).await;
                } else {
                    let _ = tx.send(
                        serde_json::json!({
                            "type": "error",
                            "code": "container_not_connected",
                            "message": "Container is not connected for question answers"
                        })
                        .to_string(),
                    );
                }
            }
            ClientMessage::EditMessage {
                message_id,
                content,
            } => {
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
                let all_msgs = db::messages::list_messages(
                    &state.db,
                    &conv_id,
                    super::WS_MAX_HISTORY_MESSAGES,
                    0,
                )
                .await
                .unwrap_or_default();
                let keep_turns = all_msgs
                    .iter()
                    .take_while(|m| m.id != msg.id)
                    .filter(|m| m.role == "user")
                    .count();

                let message_updated = match db::messages::update_message_content(
                    &state.db, &msg.id, &content,
                )
                .await
                {
                    Ok(updated) => updated,
                    Err(e) => {
                        tracing::error!(
                            conversation_id = %conv_id,
                            message_id = %msg.id,
                            error = %e,
                            "Failed to update user message"
                        );
                        false
                    }
                };
                let message_parts_updated = match db::messages_v2::upsert_message_text_part(
                    &state.db, &msg.id, &conv_id, "user", &content,
                )
                .await
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!(
                            conversation_id = %conv_id,
                            message_id = %msg.id,
                            error = %e,
                            "Failed to update user message parts in messages_v2"
                        );
                        false
                    }
                };
                let deleted_messages =
                    match db::messages::delete_messages_after(&state.db, &conv_id, &msg.id).await {
                        Ok(_) => true,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conv_id,
                                message_id = %msg.id,
                                error = %e,
                                "Failed to delete trailing messages after edit"
                            );
                            false
                        }
                    };
                let deleted_messages_v2 =
                    match db::messages_v2::delete_messages_v2_after(&state.db, &conv_id, &msg.id)
                        .await
                    {
                        Ok(_) => true,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conv_id,
                                message_id = %msg.id,
                                error = %e,
                                "Failed to delete trailing messages_v2 after edit"
                            );
                            false
                        }
                    };

                let mutation_succeeded =
                    should_touch_after_edit(message_updated, deleted_messages, deleted_messages_v2)
                        && message_parts_updated;
                if !mutation_succeeded {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        message_id = %msg.id,
                        message_updated,
                        message_parts_updated,
                        deleted_messages,
                        deleted_messages_v2,
                        "Edit mutation incomplete; notifying client"
                    );
                    let _ = tx.send(
                        serde_json::json!({
                            "type": "error",
                            "code": "edit_failed",
                            "message": "Failed to apply edit. Please try again."
                        })
                        .to_string(),
                    );
                    continue;
                }

                if let Err(e) =
                    db::conversations::touch_conversation_activity(&state.db, &conv_id, &user_id)
                        .await
                {
                    tracing::error!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to touch conversation activity after edit_message"
                    );
                }

                let updated_parts =
                    match db::messages_v2::list_message_parts(&state.db, &msg.id).await {
                        Ok(parts) => Some(ws_parts_from_db_parts(&parts)),
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conv_id,
                                message_id = %msg.id,
                                error = %e,
                                "Failed to fetch updated message parts after edit_message"
                            );
                            None
                        }
                    };
                let mut payload = serde_json::json!({
                    "type": "messages_truncated",
                    "after_message_id": msg.id,
                    "updated_content": content,
                });
                if let Some(parts) = updated_parts
                    && let Some(obj) = payload.as_object_mut()
                {
                    obj.insert("updated_parts".to_string(), serde_json::Value::Array(parts));
                }
                let _ = tx.send(payload.to_string());

                let edit_conv = db::conversations::get_conversation(&state.db, &conv_id, &user_id)
                    .await
                    .ok()
                    .flatten();
                let deep_thinking = edit_conv.as_ref().map(|c| c.deep_thinking).unwrap_or(false);
                let thinking_budget = edit_conv.as_ref().and_then(|c| c.thinking_budget);
                let subagent_thinking_budget =
                    edit_conv.as_ref().and_then(|c| c.subagent_thinking_budget);

                // Tell the running container to truncate its in-memory history
                ws_state
                    .send_to_container(
                        &conv_id,
                        &serde_json::json!({
                            "type": "truncate_history",
                            "keep_turns": keep_turns,
                        })
                        .to_string(),
                    )
                    .await;

                send_to_container_or_start(
                    &ws_state,
                    &docker_manager,
                    &tx,
                    &conv_id,
                    &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": msg.id,
                        "content": content,
                        "deep_thinking": deep_thinking,
                        "thinking_budget": thinking_budget,
                        "subagent_thinking_budget": subagent_thinking_budget,
                    })
                    .to_string(),
                )
                .await;
            }
            ClientMessage::Regenerate { message_id } => {
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
                let all_msgs = db::messages::list_messages(
                    &state.db,
                    &conv_id,
                    super::WS_MAX_HISTORY_MESSAGES,
                    0,
                )
                .await
                .unwrap_or_default();
                let msg_idx = all_msgs.iter().position(|m| m.id == msg.id);
                let last_user_msg =
                    msg_idx.and_then(|idx| all_msgs[..idx].iter().rev().find(|m| m.role == "user"));

                let user_msg = match last_user_msg {
                    Some(m) => m.clone(),
                    None => {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "error",
                                "code": "invalid_message",
                                "message": "No preceding user message found for regenerate"
                            })
                            .to_string(),
                        );
                        continue;
                    }
                };

                // Delete the assistant message and everything after the user message
                let keep_turns = all_msgs
                    .iter()
                    .take_while(|m| m.id != user_msg.id)
                    .filter(|m| m.role == "user")
                    .count();

                let deleted_messages =
                    match db::messages::delete_messages_after(&state.db, &conv_id, &user_msg.id)
                        .await
                    {
                        Ok(_) => true,
                        Err(e) => {
                            tracing::error!(
                                conversation_id = %conv_id,
                                message_id = %user_msg.id,
                                error = %e,
                                "Failed to delete trailing messages after regenerate"
                            );
                            false
                        }
                    };
                let deleted_messages_v2 = match db::messages_v2::delete_messages_v2_after(
                    &state.db,
                    &conv_id,
                    &user_msg.id,
                )
                .await
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!(
                            conversation_id = %conv_id,
                            message_id = %user_msg.id,
                            error = %e,
                            "Failed to delete trailing messages_v2 after regenerate"
                        );
                        false
                    }
                };
                let mutation_succeeded =
                    should_touch_after_regenerate(deleted_messages, deleted_messages_v2);
                if !mutation_succeeded {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        message_id = %user_msg.id,
                        deleted_messages,
                        deleted_messages_v2,
                        "Regenerate mutation incomplete; notifying client"
                    );
                    let _ = tx.send(
                        serde_json::json!({
                            "type": "error",
                            "code": "regenerate_failed",
                            "message": "Failed to regenerate response. Please try again."
                        })
                        .to_string(),
                    );
                    continue;
                }

                if let Err(e) =
                    db::conversations::touch_conversation_activity(&state.db, &conv_id, &user_id)
                        .await
                {
                    tracing::error!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to touch conversation activity after regenerate"
                    );
                }

                let _ = tx.send(
                    serde_json::json!({
                        "type": "messages_truncated",
                        "after_message_id": user_msg.id,
                    })
                    .to_string(),
                );

                let regen_conv = db::conversations::get_conversation(&state.db, &conv_id, &user_id)
                    .await
                    .ok()
                    .flatten();
                let deep_thinking = regen_conv
                    .as_ref()
                    .map(|c| c.deep_thinking)
                    .unwrap_or(false);
                let thinking_budget = regen_conv.as_ref().and_then(|c| c.thinking_budget);
                let subagent_thinking_budget =
                    regen_conv.as_ref().and_then(|c| c.subagent_thinking_budget);

                // Tell the running container to truncate its in-memory history
                ws_state
                    .send_to_container(
                        &conv_id,
                        &serde_json::json!({
                            "type": "truncate_history",
                            "keep_turns": keep_turns,
                        })
                        .to_string(),
                    )
                    .await;

                send_to_container_or_start(
                    &ws_state,
                    &docker_manager,
                    &tx,
                    &conv_id,
                    &user_id,
                    &serde_json::json!({
                        "type": "user_message",
                        "message_id": user_msg.id,
                        "content": user_msg.content,
                        "deep_thinking": deep_thinking,
                        "thinking_budget": thinking_budget,
                        "subagent_thinking_budget": subagent_thinking_budget,
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

#[cfg(test)]
mod tests {
    use super::{
        extract_ws_access_token, should_touch_after_edit, should_touch_after_regenerate,
        validate_question_answer_payload, ws_origin_allowed,
    };
    use axum::http::{HeaderMap, HeaderValue, header};

    #[test]
    fn ws_token_prefers_bearer_header_over_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer header-token"),
        );
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("access_token=cookie-token"),
        );

        let token = extract_ws_access_token(&headers);
        assert_eq!(token.as_deref(), Some("header-token"));
    }

    #[test]
    fn ws_token_falls_back_to_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("foo=bar; access_token=cookie-token"),
        );

        let token = extract_ws_access_token(&headers);
        assert_eq!(token.as_deref(), Some("cookie-token"));
    }

    #[test]
    fn ws_origin_allowed_accepts_configured_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://app.example.com"),
        );

        assert!(ws_origin_allowed(
            &headers,
            &Some("https://app.example.com,https://other.example.com".to_string())
        ));
    }

    #[test]
    fn ws_origin_allowed_rejects_unlisted_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example.com"),
        );

        assert!(!ws_origin_allowed(
            &headers,
            &Some("https://app.example.com".to_string())
        ));
    }

    #[test]
    fn ws_origin_allowed_when_not_configured() {
        let headers = HeaderMap::new();
        assert!(ws_origin_allowed(&headers, &None));
    }

    #[test]
    fn ws_origin_allowed_with_configured_list_and_missing_origin_header() {
        let headers = HeaderMap::new();
        assert!(ws_origin_allowed(
            &headers,
            &Some("https://app.example.com".to_string())
        ));
    }

    #[test]
    fn touch_after_edit_requires_all_mutations_successful() {
        assert!(should_touch_after_edit(true, true, true));
        assert!(!should_touch_after_edit(false, true, true));
        assert!(!should_touch_after_edit(true, false, true));
        assert!(!should_touch_after_edit(true, true, false));
    }

    #[test]
    fn touch_after_regenerate_requires_all_deletions_successful() {
        assert!(should_touch_after_regenerate(true, true));
        assert!(!should_touch_after_regenerate(false, true));
        assert!(!should_touch_after_regenerate(true, false));
    }

    #[test]
    fn validate_question_answer_rejects_empty_questionnaire_id() {
        let answers = serde_json::json!([{"id":"q1"}]);
        let err = validate_question_answer_payload(" ", &answers).unwrap_err();
        assert_eq!(err, "Missing questionnaire_id");
    }

    #[test]
    fn validate_question_answer_rejects_non_array_answers() {
        let answers = serde_json::json!({"id":"q1"});
        let err = validate_question_answer_payload("qq-1", &answers).unwrap_err();
        assert_eq!(err, "answers must be an array");
    }

    #[test]
    fn validate_question_answer_accepts_valid_payload() {
        let answers = serde_json::json!([{"id":"q1","selected_options":["A"]}]);
        assert!(validate_question_answer_payload("qq-1", &answers).is_ok());
    }
}
