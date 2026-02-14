use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use validator::Validate;

use crate::auth::middleware::{AdminOnly, AppState};
use crate::db;
use crate::error::AppError;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp-servers", get(list_mcp_servers).post(create_mcp_server))
        .route("/mcp-servers/{id}", get(get_mcp_server).put(update_mcp_server).delete(delete_mcp_server))
}

#[derive(Serialize)]
pub struct McpServerDetailResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<String>,
    pub url: Option<String>,
    pub env_vars: Option<String>,
    pub is_enabled: bool,
    pub created_at: String,
}

impl From<db::mcp_servers::McpServer> for McpServerDetailResponse {
    fn from(s: db::mcp_servers::McpServer) -> Self {
        Self {
            id: s.id,
            name: s.name,
            description: s.description,
            transport: s.transport,
            command: s.command,
            args: s.args,
            url: s.url,
            env_vars: s.env_vars,
            is_enabled: s.is_enabled,
            created_at: s.created_at,
        }
    }
}

async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
    _admin: AdminOnly,
) -> Result<Json<Vec<McpServerDetailResponse>>, AppError> {
    let servers = db::mcp_servers::list_mcp_servers(&state.db).await?;
    Ok(Json(servers.into_iter().map(Into::into).collect()))
}

fn validate_transport(transport: &str) -> Result<(), validator::ValidationError> {
    if transport == "stdio" || transport == "sse" {
        Ok(())
    } else {
        Err(validator::ValidationError::new("invalid_transport")
            .with_message("Transport must be 'stdio' or 'sse'".into()))
    }
}

#[derive(Deserialize, Validate)]
pub struct CreateMcpServerRequest {
    #[validate(length(min = 1, message = "Name is required"))]
    pub name: String,
    pub description: Option<String>,
    #[validate(custom(function = "validate_transport"))]
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<String>,
    pub url: Option<String>,
    pub env_vars: Option<String>,
}

async fn create_mcp_server(
    State(state): State<Arc<AppState>>,
    _admin: AdminOnly,
    Json(req): Json<CreateMcpServerRequest>,
) -> Result<(StatusCode, Json<McpServerDetailResponse>), AppError> {
    req.validate().map_err(|e| AppError::BadRequest(e.to_string()))?;

    let server = db::mcp_servers::create_mcp_server(
        &state.db,
        &req.name,
        req.description.as_deref(),
        &req.transport,
        req.command.as_deref(),
        req.args.as_deref(),
        req.url.as_deref(),
        req.env_vars.as_deref(),
        true,
    ).await?;

    Ok((StatusCode::CREATED, Json(server.into())))
}

async fn get_mcp_server(
    State(state): State<Arc<AppState>>,
    _admin: AdminOnly,
    Path(id): Path<String>,
) -> Result<Json<McpServerDetailResponse>, AppError> {
    let server = db::mcp_servers::get_mcp_server(&state.db, &id).await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(server.into()))
}

#[derive(Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub args: Option<String>,
    pub url: Option<String>,
    pub env_vars: Option<String>,
    pub is_enabled: Option<bool>,
}

async fn update_mcp_server(
    State(state): State<Arc<AppState>>,
    _admin: AdminOnly,
    Path(id): Path<String>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> Result<Json<McpServerDetailResponse>, AppError> {
    let existing = db::mcp_servers::get_mcp_server(&state.db, &id).await?
        .ok_or(AppError::NotFound)?;

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let transport = req.transport.as_deref().unwrap_or(&existing.transport);
    let is_enabled = req.is_enabled.unwrap_or(existing.is_enabled);

    let server = db::mcp_servers::update_mcp_server(
        &state.db,
        &id,
        name,
        req.description.as_deref().or(existing.description.as_deref()),
        transport,
        req.command.as_deref().or(existing.command.as_deref()),
        req.args.as_deref().or(existing.args.as_deref()),
        req.url.as_deref().or(existing.url.as_deref()),
        req.env_vars.as_deref().or(existing.env_vars.as_deref()),
        is_enabled,
    ).await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(server.into()))
}

async fn delete_mcp_server(
    State(state): State<Arc<AppState>>,
    _admin: AdminOnly,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if db::mcp_servers::delete_mcp_server(&state.db, &id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
