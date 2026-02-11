use axum::{Router, routing::get};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod api;
mod auth;
mod config;
mod crypto;
mod db;
mod docker;
mod prompts;
mod ws;

use auth::middleware::AppState;
use docker::registry::ContainerRegistry;
use ws::WsState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = config::Config::from_env();
    let pool = db::init_db(&config.database_url).await;

    let state = Arc::new(AppState {
        db: pool,
        config: config.clone(),
    });

    let ws_state = WsState::new();
    let container_registry = ContainerRegistry::new();

    // Docker manager for container lifecycle
    let docker_manager = Arc::new(docker::manager::DockerManager::new(
        config.clone(),
        container_registry.clone(),
    ));

    // Spawn idle container cleanup task (check every 30 seconds)
    docker::manager::spawn_idle_cleanup(docker_manager.clone(), 30);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Main API router (frontend-facing)
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/ws", get(ws::client::ws_handler))
        .nest("/api/auth", api::auth::router())
        .nest("/api/users", api::users::router())
        .nest("/api/conversations", api::conversations::router())
        .nest("/api/admin", api::admin::router())
        .nest("/api/mcp-servers", mcp_servers_public_router())
        .nest("/api/presets", api::presets::router())
        .layer(axum::Extension(state.clone()))
        .layer(axum::Extension(ws_state.clone()))
        .layer(axum::Extension(docker_manager.clone()))
        .layer(cors.clone())
        .layer(TraceLayer::new_for_http());

    // Internal WS router (container-facing)
    let internal_app = Router::new()
        .route("/internal/ws", get(ws::container::container_ws_handler))
        .layer(axum::Extension(state.clone()))
        .layer(axum::Extension(ws_state))
        .layer(TraceLayer::new_for_http());

    // Start both servers
    let main_addr = format!("0.0.0.0:{}", config.port);
    let internal_addr = format!("0.0.0.0:{}", config.internal_ws_port);

    tracing::info!("Backend API listening on {main_addr}");
    tracing::info!("Internal WS listening on {internal_addr}");

    let main_listener = tokio::net::TcpListener::bind(&main_addr).await.unwrap();
    let internal_listener = tokio::net::TcpListener::bind(&internal_addr).await.unwrap();

    tokio::select! {
        r = axum::serve(main_listener, app) => {
            if let Err(e) = r { tracing::error!("Main server error: {e}"); }
        }
        r = axum::serve(internal_listener, internal_app) => {
            if let Err(e) = r { tracing::error!("Internal server error: {e}"); }
        }
    }
}

async fn health() -> &'static str {
    "ok"
}

fn mcp_servers_public_router() -> Router {
    Router::new().route("/", get(list_available_mcp_servers))
}

async fn list_available_mcp_servers(
    axum::extract::Extension(state): axum::extract::Extension<Arc<AppState>>,
    _auth: auth::middleware::AuthUser,
) -> axum::Json<Vec<api::conversations::McpServerResponse>> {
    let servers = db::mcp_servers::list_enabled_mcp_servers(&state.db).await;
    axum::Json(
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
    )
}
