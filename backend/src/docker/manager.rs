use std::sync::Arc;

use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::models::HostConfig;
use bollard::Docker;

use super::registry::ContainerRegistry;
use crate::auth;
use crate::config;

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

    /// Start a container for a conversation. Returns the container ID.
    pub async fn start_container(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<String, String> {
        // Check if already running
        if let Some(info) = self.registry.get(conversation_id).await {
            self.registry.touch(conversation_id).await;
            return Ok(info.container_id);
        }

        // Generate container token
        let container_token =
            auth::create_container_token(conversation_id, user_id, &self.config.jwt_secret)
                .map_err(|e| format!("Failed to create container token: {e}"))?;

        let backend_ws_url = format!(
            "ws://host.docker.internal:{}/internal/ws",
            self.config.internal_ws_port
        );

        let workspace_path =
            std::fs::canonicalize(format!("data/conversations/{conversation_id}"))
                .unwrap_or_else(|_| {
                    let path = format!("data/conversations/{conversation_id}");
                    std::fs::create_dir_all(&path).ok();
                    std::path::PathBuf::from(&path)
                });

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
                binds: Some(vec![format!(
                    "{}:/workspace",
                    workspace_path.to_string_lossy()
                )]),
                extra_hosts: Some(vec![
                    "host.docker.internal:host-gateway".to_string(),
                ]),
                memory: Some(512 * 1024 * 1024), // 512MB
                nano_cpus: Some(1_000_000_000),   // 1 CPU
                ..Default::default()
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
            .await
            .map_err(|e| format!("Failed to create container: {e}"))?;

        let container_id = create_result.id;

        // Start container
        self.docker
            .start_container(&container_id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| format!("Failed to start container: {e}"))?;

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
    pub async fn stop_container(&self, conversation_id: &str) -> Result<(), String> {
        let info = self
            .registry
            .unregister(conversation_id)
            .await
            .ok_or_else(|| "Container not found in registry".to_string())?;

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
