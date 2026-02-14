pub mod client;
pub mod container;
pub mod messages;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Maximum number of messages to fetch for WS history operations.
pub const WS_MAX_HISTORY_MESSAGES: i64 = 1000;
/// Number of recent messages to send to a container on init.
pub const CONTAINER_INIT_HISTORY_LIMIT: i64 = 50;

pub type WsSender = mpsc::UnboundedSender<String>;

#[derive(Default)]
pub struct WsState {
    pub client_connections: RwLock<HashMap<String, HashMap<String, WsSender>>>,
    pub container_connections: RwLock<HashMap<String, (WsSender, u64)>>,
    /// Messages queued while a container was starting (keyed by conversation_id).
    pub pending_messages: RwLock<HashMap<String, String>>,
    /// Monotonically increasing generation counter for container connections.
    container_gen: AtomicU64,
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

    pub async fn add_container(&self, conversation_id: &str, sender: WsSender) -> u64 {
        let generation = self.container_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let mut conns = self.container_connections.write().await;
        conns.insert(conversation_id.to_string(), (sender, generation));
        generation
    }

    pub async fn remove_container(&self, conversation_id: &str) {
        let mut conns = self.container_connections.write().await;
        conns.remove(conversation_id);
    }

    /// Remove the container connection only if the stored generation matches.
    /// Returns `true` if removed, `false` if a newer connection has replaced it.
    pub async fn remove_container_if_gen(&self, conversation_id: &str, generation: u64) -> bool {
        let mut conns = self.container_connections.write().await;
        if let Some((_, stored_gen)) = conns.get(conversation_id) {
            if *stored_gen == generation {
                conns.remove(conversation_id);
                return true;
            }
        }
        false
    }

    pub async fn send_to_container(&self, conversation_id: &str, msg: &str) -> bool {
        let conns = self.container_connections.read().await;
        if let Some((sender, _)) = conns.get(conversation_id) {
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

    #[tokio::test]
    async fn test_set_and_take_pending_message() {
        let state = WsState::new();

        // No pending message initially
        assert!(state.take_pending_message("conv1").await.is_none());

        // Set a pending message
        state.set_pending_message("conv1", "init msg".to_string()).await;

        // Take it — should return the message
        let msg = state.take_pending_message("conv1").await;
        assert_eq!(msg.as_deref(), Some("init msg"));

        // Take again — should be gone
        assert!(state.take_pending_message("conv1").await.is_none());
    }

    #[tokio::test]
    async fn test_pending_message_overwrite() {
        let state = WsState::new();

        state.set_pending_message("conv1", "first".to_string()).await;
        state.set_pending_message("conv1", "second".to_string()).await;

        let msg = state.take_pending_message("conv1").await;
        assert_eq!(msg.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn test_pending_messages_isolated_by_conversation() {
        let state = WsState::new();

        state.set_pending_message("conv1", "msg1".to_string()).await;
        state.set_pending_message("conv2", "msg2".to_string()).await;

        assert_eq!(state.take_pending_message("conv1").await.as_deref(), Some("msg1"));
        assert_eq!(state.take_pending_message("conv2").await.as_deref(), Some("msg2"));
    }

    #[tokio::test]
    async fn test_remove_container_if_gen_matching() {
        let state = WsState::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        let generation = state.add_container("conv1", tx).await;
        assert!(state.remove_container_if_gen("conv1", generation).await);
        assert!(!state.send_to_container("conv1", "ping").await);
    }

    #[tokio::test]
    async fn test_remove_container_if_gen_stale() {
        let state = WsState::new();

        let (tx_old, _rx_old) = mpsc::unbounded_channel();
        let gen_old = state.add_container("conv1", tx_old).await;

        // New container replaces old one (simulates model switch + new container start)
        let (tx_new, mut rx_new) = mpsc::unbounded_channel();
        let gen_new = state.add_container("conv1", tx_new).await;
        assert_ne!(gen_old, gen_new);

        // Old container cleanup with stale generation — must NOT remove new sender
        assert!(!state.remove_container_if_gen("conv1", gen_old).await);

        // New container's sender still works
        assert!(state.send_to_container("conv1", "hello").await);
        assert_eq!(rx_new.recv().await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_remove_container_if_gen_nonexistent() {
        let state = WsState::new();
        assert!(!state.remove_container_if_gen("noconv", 1).await);
    }

    #[tokio::test]
    async fn test_generation_increments() {
        let state = WsState::new();

        let (tx1, _) = mpsc::unbounded_channel();
        let (tx2, _) = mpsc::unbounded_channel();
        let (tx3, _) = mpsc::unbounded_channel();

        let g1 = state.add_container("a", tx1).await;
        let g2 = state.add_container("b", tx2).await;
        let g3 = state.add_container("a", tx3).await;

        assert_eq!(g1, 1);
        assert_eq!(g2, 2);
        assert_eq!(g3, 3);
    }
}
