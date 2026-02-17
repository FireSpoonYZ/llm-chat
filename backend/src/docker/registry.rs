use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tracks running containers and their last activity time.
#[derive(Default)]
pub struct ContainerRegistry {
    /// conversation_id -> ContainerInfo
    containers: RwLock<HashMap<String, ContainerInfo>>,
}

#[derive(Clone, Debug)]
pub struct ContainerInfo {
    pub container_id: String,
    pub conversation_id: String,
    #[allow(dead_code)]
    pub user_id: String,
    pub last_activity: std::time::Instant,
}

impl ContainerRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn register(&self, conversation_id: &str, container_id: &str, user_id: &str) {
        let mut containers = self.containers.write().await;
        containers.insert(
            conversation_id.to_string(),
            ContainerInfo {
                container_id: container_id.to_string(),
                conversation_id: conversation_id.to_string(),
                user_id: user_id.to_string(),
                last_activity: std::time::Instant::now(),
            },
        );
    }

    pub async fn unregister(&self, conversation_id: &str) -> Option<ContainerInfo> {
        let mut containers = self.containers.write().await;
        containers.remove(conversation_id)
    }

    pub async fn touch(&self, conversation_id: &str) {
        let mut containers = self.containers.write().await;
        if let Some(info) = containers.get_mut(conversation_id) {
            info.last_activity = std::time::Instant::now();
        }
    }

    pub async fn get(&self, conversation_id: &str) -> Option<ContainerInfo> {
        let containers = self.containers.read().await;
        containers.get(conversation_id).cloned()
    }

    pub async fn get_idle_containers(&self, timeout_secs: u64) -> Vec<ContainerInfo> {
        let containers = self.containers.read().await;
        let threshold = std::time::Duration::from_secs(timeout_secs);
        containers
            .values()
            .filter(|info| info.last_activity.elapsed() > threshold)
            .cloned()
            .collect()
    }

    pub async fn list_all(&self) -> Vec<ContainerInfo> {
        let containers = self.containers.read().await;
        containers.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "container_abc", "user1").await;

        let info = registry.get("conv1").await;
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.container_id, "container_abc");
        assert_eq!(info.user_id, "user1");
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "container_abc", "user1").await;

        let removed = registry.unregister("conv1").await;
        assert!(removed.is_some());

        let info = registry.get("conv1").await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let registry = ContainerRegistry::new();
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_touch_updates_activity() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "container_abc", "user1").await;

        let before = registry.get("conv1").await.unwrap().last_activity;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        registry.touch("conv1").await;
        let after = registry.get("conv1").await.unwrap().last_activity;

        assert!(after > before);
    }

    #[tokio::test]
    async fn test_get_idle_containers() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "c1", "user1").await;

        // With a 0-second timeout, everything is idle
        let idle = registry.get_idle_containers(0).await;
        assert_eq!(idle.len(), 1);

        // With a very large timeout, nothing is idle
        let idle = registry.get_idle_containers(999999).await;
        assert_eq!(idle.len(), 0);
    }

    #[tokio::test]
    async fn test_list_all() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "c1", "user1").await;
        registry.register("conv2", "c2", "user2").await;

        let all = registry.list_all().await;
        assert_eq!(all.len(), 2);
    }
}
