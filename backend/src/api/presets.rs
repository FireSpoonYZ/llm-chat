use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::auth::middleware::{AppState, AuthUser};
use crate::db;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_presets).post(create_preset))
        .route("/{id}", axum::routing::put(update_preset).delete(delete_preset))
}

async fn list_presets(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
) -> Json<Vec<db::presets::UserPreset>> {
    db::presets::seed_builtin_presets(&state.db, &auth.user_id).await;
    let presets = db::presets::list_presets(&state.db, &auth.user_id).await;
    Json(presets)
}

#[derive(Deserialize)]
pub struct CreatePresetRequest {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub is_default: Option<bool>,
}

async fn create_preset(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreatePresetRequest>,
) -> Result<(StatusCode, Json<db::presets::UserPreset>), StatusCode> {
    let preset = db::presets::create_preset(
        &state.db,
        &auth.user_id,
        &req.name,
        req.description.as_deref().unwrap_or(""),
        &req.content,
        req.is_default.unwrap_or(false),
    )
    .await;
    Ok((StatusCode::CREATED, Json(preset)))
}

#[derive(Deserialize)]
pub struct UpdatePresetRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub is_default: Option<bool>,
}

async fn update_preset(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdatePresetRequest>,
) -> Result<Json<db::presets::UserPreset>, StatusCode> {
    let preset = db::presets::update_preset(
        &state.db,
        &id,
        &auth.user_id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.content.as_deref(),
        req.is_default,
    )
    .await
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(preset))
}

async fn delete_preset(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> StatusCode {
    if db::presets::delete_preset(&state.db, &id, &auth.user_id).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
