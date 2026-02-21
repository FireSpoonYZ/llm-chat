use std::collections::HashMap;
use std::sync::Arc;

use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, NetworkingConfig, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::models::{EndpointSettings, HostConfig};
use dashmap::DashMap;
use tokio::sync::Mutex;

use super::registry::ContainerRegistry;
use crate::auth;
use crate::config;
use crate::ws::WsState;

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("failed to create container token: {0}")]
    TokenCreation(#[from] jsonwebtoken::errors::Error),
    #[error("Docker API error: {0}")]
    Bollard(#[from] bollard::errors::Error),
    #[error("{0}")]
    Other(String),
}

pub struct DockerManager {
    docker: Docker,
    registry: Arc<ContainerRegistry>,
    config: config::Config,
    /// Per-conversation lock to prevent TOCTOU races in start_container.
    start_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl DockerManager {
    pub fn new(config: config::Config, registry: Arc<ContainerRegistry>) -> Self {
        let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");
        Self {
            docker,
            registry,
            config,
            start_locks: DashMap::new(),
        }
    }

    /// Create a DockerManager for testing (does not panic if Docker socket is missing).
    #[cfg(test)]
    pub fn new_for_test(config: config::Config, registry: Arc<ContainerRegistry>) -> Self {
        let docker = Docker::connect_with_local_defaults()
            .or_else(|_| Docker::connect_with_defaults())
            .expect("Failed to create Docker client for test");
        Self {
            docker,
            registry,
            config,
            start_locks: DashMap::new(),
        }
    }

    /// Start a container for a conversation. Returns the container ID.
    pub async fn start_container(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<String, DockerError> {
        // Acquire per-conversation lock to prevent TOCTOU races where two
        // concurrent callers both pass the registry.get() check and create
        // duplicate containers.
        let lock = self
            .start_locks
            .entry(conversation_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        // Check if already running (under lock)
        if let Some(info) = self.registry.get(conversation_id).await {
            self.registry.touch(conversation_id).await;
            return Ok(info.container_id);
        }

        // Generate container token
        let container_token = auth::create_container_token(
            conversation_id,
            user_id,
            &self.config.jwt_secret,
            self.config.container_token_ttl_secs,
        )?;

        let backend_ws_url = if self.config.docker_network.is_some() {
            format!("ws://backend:{}/internal/ws", self.config.internal_ws_port)
        } else {
            format!(
                "ws://host.docker.internal:{}/internal/ws",
                self.config.internal_ws_port
            )
        };

        let container_data_path = format!("data/conversations/{conversation_id}");
        tokio::fs::create_dir_all(&container_data_path).await.ok();

        let workspace_host_path = if let Some(ref host_dir) = self.config.host_data_dir {
            format!("{}/conversations/{}", host_dir, conversation_id)
        } else {
            tokio::fs::canonicalize(&container_data_path)
                .await
                .unwrap_or_else(|_| std::path::PathBuf::from(&container_data_path))
                .to_string_lossy()
                .to_string()
        };

        let container_name = format!(
            "claude-chat-agent-{}",
            conversation_id.get(..8).unwrap_or(conversation_id)
        );

        // Remove any existing container with the same name (e.g. from a previous crash)
        let _ = self
            .docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        let container_config = Config {
            image: Some(self.config.container_image.clone()),
            env: Some(vec![
                format!("BACKEND_WS_URL={backend_ws_url}"),
                format!("CONTAINER_TOKEN={container_token}"),
                format!("CONVERSATION_ID={conversation_id}"),
            ]),
            host_config: Some(HostConfig {
                binds: Some(vec![format!("{}:/workspace", workspace_host_path)]),
                extra_hosts: if self.config.docker_network.is_none() {
                    Some(vec!["host.docker.internal:host-gateway".to_string()])
                } else {
                    None
                },
                memory: Some(512 * 1024 * 1024), // 512MB
                nano_cpus: Some(1_000_000_000),  // 1 CPU
                ..Default::default()
            }),
            networking_config: self.config.docker_network.as_ref().map(|network| {
                NetworkingConfig {
                    endpoints_config: HashMap::from([(
                        network.clone(),
                        EndpointSettings::default(),
                    )]),
                }
            }),
            working_dir: Some("/workspace".to_string()),
            ..Default::default()
        };

        // Create container
        let create_result = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: &container_name,
                    platform: None,
                }),
                container_config,
            )
            .await?;

        let container_id = create_result.id;

        // Start container
        self.docker
            .start_container(&container_id, None::<StartContainerOptions<String>>)
            .await?;

        // Register in registry
        self.registry
            .register(conversation_id, &container_id, user_id)
            .await;

        tracing::info!(
            "Started container {} for conversation {}",
            container_id,
            conversation_id
        );

        Ok(container_id)
    }

    /// Stop and remove a container for a conversation.
    pub async fn stop_container(&self, conversation_id: &str) -> Result<(), DockerError> {
        let info = self
            .registry
            .unregister(conversation_id)
            .await
            .ok_or_else(|| DockerError::Other("Container not found in registry".into()))?;

        // Stop container
        let _ = self
            .docker
            .stop_container(&info.container_id, Some(StopContainerOptions { t: 10 }))
            .await;

        // Remove container
        let _ = self
            .docker
            .remove_container(
                &info.container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        tracing::info!(
            "Stopped container {} for conversation {}",
            info.container_id,
            conversation_id
        );

        Ok(())
    }

    /// Refresh the last-activity timestamp for a conversation's container.
    pub async fn touch_activity(&self, conversation_id: &str) {
        self.registry.touch(conversation_id).await;
    }

    /// Stop idle containers that have exceeded the timeout.
    pub async fn cleanup_idle_containers(&self, ws_state: &WsState) {
        let idle = self
            .registry
            .get_idle_containers(self.config.container_idle_timeout_secs)
            .await;

        if idle.is_empty() {
            return;
        }

        // Remove from WsState AND registry before touching Docker, so any
        // concurrent send_to_container returns false AND start_container
        // creates a fresh container instead of returning the stale one.
        for info in &idle {
            tracing::info!(
                "Stopping idle container {} for conversation {}",
                info.container_id,
                info.conversation_id
            );
            ws_state.remove_container(&info.conversation_id).await;
            self.registry.unregister(&info.conversation_id).await;
        }

        // Stop and remove containers in parallel.
        let futs = idle.into_iter().map(|info| {
            let docker = self.docker.clone();
            async move {
                let _ = docker
                    .stop_container(&info.container_id, Some(StopContainerOptions { t: 10 }))
                    .await;
                let _ = docker
                    .remove_container(
                        &info.container_id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        });
        futures_util::future::join_all(futs).await;
    }

    /// List all running containers.
    #[allow(dead_code)]
    pub async fn list_containers(&self) -> Vec<super::registry::ContainerInfo> {
        self.registry.list_all().await
    }

    /// Stop and remove all running containers (used during graceful shutdown).
    pub async fn shutdown(&self) {
        let containers = self.registry.list_all().await;
        if containers.is_empty() {
            return;
        }
        tracing::info!("Shutting down {} container(s)...", containers.len());
        for info in containers {
            let _ = self.stop_container(&info.conversation_id).await;
        }
        tracing::info!("All containers stopped");
    }
}

/// Spawn a background task that periodically cleans up idle containers.
pub fn spawn_idle_cleanup(manager: Arc<DockerManager>, ws_state: Arc<WsState>, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            manager.cleanup_idle_containers(&ws_state).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_touch_activity_updates_registry() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "container_abc", "user1").await;

        let before = registry.get("conv1").await.unwrap().last_activity;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let config = config::Config::from_env();
        let manager = DockerManager::new_for_test(config, registry.clone());
        manager.touch_activity("conv1").await;

        let after = registry.get("conv1").await.unwrap().last_activity;
        assert!(after > before);
    }

    #[tokio::test]
    async fn test_touch_activity_nonexistent_is_noop() {
        let registry = ContainerRegistry::new();
        let config = config::Config::from_env();
        let manager = DockerManager::new_for_test(config, registry);
        // Should not panic
        manager.touch_activity("nonexistent").await;
    }

    #[tokio::test]
    async fn test_cleanup_idle_removes_ws_state_and_registry() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "c1", "user1").await;

        let ws_state = WsState::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(crate::ws::WS_CHANNEL_CAPACITY);
        ws_state.add_container("conv1", tx).await;

        // Use 0-second timeout so everything is idle
        let mut config = config::Config::from_env();
        config.container_idle_timeout_secs = 0;
        let manager = DockerManager::new_for_test(config, registry.clone());

        manager.cleanup_idle_containers(&ws_state).await;

        // Both registry and WsState should be cleaned up
        assert!(registry.get("conv1").await.is_none());
        assert!(!ws_state.send_to_container("conv1", "ping").await);
    }

    #[tokio::test]
    async fn test_touch_activity_prevents_idle_cleanup() {
        let registry = ContainerRegistry::new();
        registry.register("conv1", "c1", "user1").await;

        let ws_state = WsState::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(crate::ws::WS_CHANNEL_CAPACITY);
        ws_state.add_container("conv1", tx).await;

        let mut config = config::Config::from_env();
        config.container_idle_timeout_secs = 999999;
        let manager = DockerManager::new_for_test(config, registry.clone());

        manager.touch_activity("conv1").await;
        manager.cleanup_idle_containers(&ws_state).await;

        // Container should still be registered (not idle)
        assert!(registry.get("conv1").await.is_some());
        assert!(ws_state.send_to_container("conv1", "ping").await);
    }
}
