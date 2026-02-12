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
