pub mod client;
pub mod container;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub type WsSender = mpsc::UnboundedSender<String>;

#[derive(Default)]
pub struct WsState {
    pub client_connections: RwLock<HashMap<String, HashMap<String, WsSender>>>,
    pub container_connections: RwLock<HashMap<String, WsSender>>,
    /// Messages queued while a container was starting (keyed by conversation_id).
    pub pending_messages: RwLock<HashMap<String, String>>,
}

impl WsState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn add_client(&self, user_id: &str, conversation_id: &str, sender: WsSender) {
        let mut conns = self.client_connections.write().await;
        conns
            .entry(user_id.to_string())
            .or_default()
            .insert(conversation_id.to_string(), sender);
    }

    pub async fn remove_client(&self, user_id: &str, conversation_id: &str) {
        let mut conns = self.client_connections.write().await;
        if let Some(user_conns) = conns.get_mut(user_id) {
            user_conns.remove(conversation_id);
            if user_conns.is_empty() {
                conns.remove(user_id);
            }
        }
    }

    pub async fn send_to_client(&self, user_id: &str, conversation_id: &str, msg: &str) {
        let conns = self.client_connections.read().await;
        if let Some(user_conns) = conns.get(user_id) {
            if let Some(sender) = user_conns.get(conversation_id) {
                let _ = sender.send(msg.to_string());
            }
        }
    }

    pub async fn add_container(&self, conversation_id: &str, sender: WsSender) {
        let mut conns = self.container_connections.write().await;
        conns.insert(conversation_id.to_string(), sender);
    }

    pub async fn remove_container(&self, conversation_id: &str) {
        let mut conns = self.container_connections.write().await;
        conns.remove(conversation_id);
    }

    pub async fn send_to_container(&self, conversation_id: &str, msg: &str) -> bool {
        let conns = self.container_connections.read().await;
        if let Some(sender) = conns.get(conversation_id) {
            sender.send(msg.to_string()).is_ok()
        } else {
            false
        }
    }

    pub async fn set_pending_message(&self, conversation_id: &str, msg: String) {
        let mut pending = self.pending_messages.write().await;
        pending.insert(conversation_id.to_string(), msg);
    }

    pub async fn take_pending_message(&self, conversation_id: &str) -> Option<String> {
        let mut pending = self.pending_messages.write().await;
        pending.remove(conversation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_remove_client() {
        let state = WsState::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        state.add_client("user1", "conv1", tx).await;

        {
            let conns = state.client_connections.read().await;
            assert!(conns.get("user1").unwrap().contains_key("conv1"));
        }

        state.remove_client("user1", "conv1").await;

        {
            let conns = state.client_connections.read().await;
            assert!(conns.get("user1").is_none());
        }
    }

    #[tokio::test]
    async fn test_send_to_client() {
        let state = WsState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        state.add_client("user1", "conv1", tx).await;
        state.send_to_client("user1", "conv1", "hello").await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg, "hello");
    }

    #[tokio::test]
    async fn test_send_to_nonexistent_client() {
        let state = WsState::new();
        // Should not panic
        state.send_to_client("nobody", "noconv", "hello").await;
    }

    #[tokio::test]
    async fn test_add_and_remove_container() {
        let state = WsState::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        state.add_container("conv1", tx).await;

        {
            let conns = state.container_connections.read().await;
            assert!(conns.contains_key("conv1"));
        }

        state.remove_container("conv1").await;

        {
            let conns = state.container_connections.read().await;
            assert!(!conns.contains_key("conv1"));
        }
    }

    #[tokio::test]
    async fn test_send_to_container() {
        let state = WsState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        state.add_container("conv1", tx).await;
        let sent = state.send_to_container("conv1", "test msg").await;
        assert!(sent);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg, "test msg");
    }

    #[tokio::test]
    async fn test_send_to_nonexistent_container() {
        let state = WsState::new();
        let sent = state.send_to_container("noconv", "hello").await;
        assert!(!sent);
    }

    #[tokio::test]
    async fn test_multiple_clients_same_user() {
        let state = WsState::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();

        state.add_client("user1", "conv1", tx1).await;
        state.add_client("user1", "conv2", tx2).await;

        state.send_to_client("user1", "conv1", "msg1").await;
        state.send_to_client("user1", "conv2", "msg2").await;

        assert_eq!(rx1.recv().await.unwrap(), "msg1");
        assert_eq!(rx2.recv().await.unwrap(), "msg2");

        // Remove one, other should still work
        state.remove_client("user1", "conv1").await;
        state.send_to_client("user1", "conv2", "msg3").await;
        assert_eq!(rx2.recv().await.unwrap(), "msg3");
    }
}
