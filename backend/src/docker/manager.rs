use std::collections::HashMap;
use std::sync::Arc;

use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions, NetworkingConfig,
};
use bollard::models::{EndpointSettings, HostConfig};
use bollard::Docker;

use super::registry::ContainerRegistry;
use crate::auth;
use crate::config;

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
}

impl DockerManager {
    pub fn new(config: config::Config, registry: Arc<ContainerRegistry>) -> Self {
        let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");
        Self {
            docker,
            registry,
            config,
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
        }
    }

    /// Start a container for a conversation. Returns the container ID.
    pub async fn start_container(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<String, DockerError> {
        // Check if already running
        if let Some(info) = self.registry.get(conversation_id).await {
            self.registry.touch(conversation_id).await;
            return Ok(info.container_id);
        }

        // Generate container token
        let container_token =
            auth::create_container_token(conversation_id, user_id, &self.config.jwt_secret, self.config.container_token_ttl_secs)?;

        let backend_ws_url = if self.config.docker_network.is_some() {
            format!(
                "ws://backend:{}/internal/ws",
                self.config.internal_ws_port
            )
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

        let container_name = format!("claude-chat-agent-{}", &conversation_id[..8]);

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
                nano_cpus: Some(1_000_000_000),   // 1 CPU
                ..Default::default()
            }),
            networking_config: if let Some(ref network) = self.config.docker_network {
                Some(NetworkingConfig {
                    endpoints_config: HashMap::from([(
                        network.clone(),
                        EndpointSettings::default(),
                    )]),
                })
            } else {
                None
            },
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
            .stop_container(
                &info.container_id,
                Some(StopContainerOptions { t: 10 }),
            )
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

    /// Stop idle containers that have exceeded the timeout.
    pub async fn cleanup_idle_containers(&self) {
        let idle = self
            .registry
            .get_idle_containers(self.config.container_idle_timeout_secs)
            .await;

        for info in idle {
            tracing::info!(
                "Stopping idle container {} for conversation {}",
                info.container_id,
                info.conversation_id
            );
            let _ = self.stop_container(&info.conversation_id).await;
        }
    }

    /// List all running containers.
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
pub fn spawn_idle_cleanup(manager: Arc<DockerManager>, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            manager.cleanup_idle_containers().await;
        }
    });
}
