use axum::{Router, extract::DefaultBodyLimit, routing::get};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use axum::http::HeaderValue;

mod api;
mod auth;
mod config;
mod crypto;
mod db;
mod docker;
mod error;
mod prompts;
mod ws;

use auth::middleware::AppState;
use ws::WsState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = config::Config::from_env();
    let pool = db::init_db(&config.database_url).await;

    let ws_state = WsState::new();

    // Docker manager for container lifecycle
    let container_registry = docker::registry::ContainerRegistry::new();
    let docker_manager = Arc::new(docker::manager::DockerManager::new(
        config.clone(),
        container_registry.clone(),
    ));

    // Spawn idle container cleanup task (check every 30 seconds)
    docker::manager::spawn_idle_cleanup(docker_manager.clone(), 30);

    let state = Arc::new(AppState {
        db: pool,
        config: config.clone(),
        ws_state: ws_state.clone(),
        docker_manager: docker_manager.clone(),
    });

    let cors = if let Some(ref origins) = config.cors_allowed_origins {
        let allowed: Vec<HeaderValue> = origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(allowed)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Main API router (frontend-facing)
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/ws", get(ws::client::ws_handler))
        .nest("/api/auth", api::auth::router())
        .nest("/api/users", api::users::router())
        .nest("/api/conversations", api::conversations::router())
        .nest(
            "/api/conversations/{id}/files",
            api::files::router().layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .nest("/api/admin", api::admin::router())
        .nest("/api/mcp-servers", mcp_servers_public_router())
        .nest("/api/presets", api::presets::router())
        .with_state(state.clone())
        .layer(cors.clone())
        .layer(TraceLayer::new_for_http());

    // Internal WS router (container-facing)
    let internal_app = Router::new()
        .route("/internal/ws", get(ws::container::container_ws_handler))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());

    // Start both servers
    let main_addr = format!("0.0.0.0:{}", config.port);
    let internal_addr = format!("0.0.0.0:{}", config.internal_ws_port);

    tracing::info!("Backend API listening on {main_addr}");
    tracing::info!("Internal WS listening on {internal_addr}");

    let main_listener = tokio::net::TcpListener::bind(&main_addr).await.unwrap();
    let internal_listener = tokio::net::TcpListener::bind(&internal_addr).await.unwrap();

    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Shutdown signal received, stopping...");
    };

    let dm = docker_manager.clone();
    tokio::select! {
        r = axum::serve(main_listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_signal) => {
            if let Err(e) = r { tracing::error!("Main server error: {e}"); }
        }
        r = axum::serve(internal_listener, internal_app) => {
            if let Err(e) = r { tracing::error!("Internal server error: {e}"); }
        }
    }

    dm.shutdown().await;
}

async fn health() -> &'static str {
    "ok"
}

fn mcp_servers_public_router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(list_available_mcp_servers))
}

async fn list_available_mcp_servers(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    _auth: auth::middleware::AuthUser,
) -> Result<axum::Json<Vec<api::conversations::McpServerResponse>>, error::AppError> {
    let servers = db::mcp_servers::list_enabled_mcp_servers(&state.db).await?;
    Ok(axum::Json(
        servers
            .into_iter()
            .map(|s| api::conversations::McpServerResponse {
                id: s.id,
                name: s.name,
                description: s.description,
                transport: s.transport,
                is_enabled: s.is_enabled,
            })
            .collect(),
    ))
}
