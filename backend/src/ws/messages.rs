use serde::Deserialize;

/// Messages sent from the frontend client to the backend via WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    JoinConversation {
        conversation_id: String,
    },
    UserMessage {
        content: String,
    },
    EditMessage {
        message_id: String,
        content: String,
    },
    Regenerate {
        message_id: String,
    },
    Cancel,
    Ping,
}

/// Messages sent from the container agent to the backend via internal WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContainerMessage {
    Ready,
    Complete {
        content: Option<String>,
        tool_calls: Option<serde_json::Value>,
        token_usage: Option<serde_json::Value>,
    },
    Error,
    /// Forwarded types: assistant_delta, thinking_delta, tool_call, tool_result.
    /// These are handled as raw JSON to preserve all fields during forwarding.
    #[serde(other)]
    Forward,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_join_conversation() {
        let json = r#"{"type": "join_conversation", "conversation_id": "conv-1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::JoinConversation { conversation_id } if conversation_id == "conv-1"));
    }

    #[test]
    fn deserialize_user_message() {
        let json = r#"{"type": "user_message", "content": "hello"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::UserMessage { content } if content == "hello"));
    }

    #[test]
    fn deserialize_edit_message() {
        let json = r#"{"type": "edit_message", "message_id": "msg-1", "content": "edited"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::EditMessage { message_id, content } if message_id == "msg-1" && content == "edited"));
    }

    #[test]
    fn deserialize_regenerate() {
        let json = r#"{"type": "regenerate", "message_id": "msg-1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Regenerate { message_id } if message_id == "msg-1"));
    }

    #[test]
    fn deserialize_cancel() {
        let json = r#"{"type": "cancel"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Cancel));
    }

    #[test]
    fn deserialize_ping() {
        let json = r#"{"type": "ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn deserialize_container_ready() {
        let json = r#"{"type": "ready"}"#;
        let msg: ContainerMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ContainerMessage::Ready));
    }

    #[test]
    fn deserialize_container_complete() {
        let json = r#"{"type": "complete", "content": "done", "tool_calls": null, "token_usage": null}"#;
        let msg: ContainerMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ContainerMessage::Complete { content: Some(c), .. } if c == "done"));
    }

    #[test]
    fn deserialize_container_error() {
        let json = r#"{"type": "error"}"#;
        let msg: ContainerMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ContainerMessage::Error));
    }

    #[test]
    fn deserialize_unknown_type_as_forward() {
        let json = r#"{"type": "assistant_delta", "content": "hi"}"#;
        let msg: ContainerMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ContainerMessage::Forward));
    }
}
