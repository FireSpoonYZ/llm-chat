use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::middleware::{AppState, AuthUser};
use crate::crypto;
use crate::db;

pub fn router() -> Router {
    Router::new()
        .route("/me", get(get_profile))
        .route("/me/providers", get(list_providers).post(upsert_provider))
        .route("/me/providers/{name}", delete(delete_provider))
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: String,
}

async fn get_profile(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ProfileResponse>, StatusCode> {
    let user = db::users::get_user_by_id(&state.db, &auth.user_id).await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ProfileResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_admin: user.is_admin,
        created_at: user.created_at,
    }))
}

#[derive(Serialize)]
pub struct ProviderResponse {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub endpoint_url: Option<String>,
    pub models: Vec<String>,
    pub is_default: bool,
    pub has_api_key: bool,
}

fn parse_models_json(json_str: Option<&str>) -> Vec<String> {
    json_str
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

async fn list_providers(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<ProviderResponse>>, StatusCode> {
    let providers = db::providers::list_providers(&state.db, &auth.user_id).await;
    Ok(Json(
        providers
            .into_iter()
            .map(|p| ProviderResponse {
                id: p.id,
                name: p.name.unwrap_or_else(|| p.provider.clone()),
                provider: p.provider,
                endpoint_url: p.endpoint_url,
                models: parse_models_json(p.models.as_deref()),
                is_default: p.is_default,
                has_api_key: true,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct UpsertProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub api_key: String,
    pub endpoint_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub is_default: bool,
}

async fn upsert_provider(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpsertProviderRequest>,
) -> Result<Json<ProviderResponse>, (StatusCode, String)> {
    let valid_providers = ["openai", "anthropic", "google", "mistral"];
    if !valid_providers.contains(&req.provider_type.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid provider type".into()));
    }

    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name is required".into()));
    }

    // If editing and keeping existing key, look up the existing encrypted key
    let encrypted_key = if req.api_key == "__KEEP_EXISTING__" {
        // Find existing provider by name to reuse its encrypted key
        let existing = db::providers::list_providers(&state.db, &auth.user_id).await
            .into_iter()
            .find(|p| p.name.as_deref() == Some(req.name.as_str()));
        match existing {
            Some(p) => p.api_key_encrypted,
            None => return Err((StatusCode::BAD_REQUEST, "API key is required for new providers".into())),
        }
    } else {
        crypto::encrypt(&req.api_key, &state.config.encryption_key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    };

    let models_json = req.models.as_ref().map(|m| serde_json::to_string(m).unwrap());
    let first_model = req.models.as_ref().and_then(|m| m.first().cloned());

    let id = uuid::Uuid::new_v4().to_string();
    let provider = db::providers::upsert_provider(
        &state.db,
        Some(&id),
        &auth.user_id,
        &req.provider_type,
        &encrypted_key,
        req.endpoint_url.as_deref(),
        first_model.as_deref(),
        req.is_default,
        models_json.as_deref(),
        Some(&req.name),
    ).await;

    Ok(Json(ProviderResponse {
        id: provider.id,
        name: provider.name.unwrap_or_else(|| provider.provider.clone()),
        provider: provider.provider,
        endpoint_url: provider.endpoint_url,
        models: parse_models_json(provider.models.as_deref()),
        is_default: provider.is_default,
        has_api_key: true,
    }))
}

async fn delete_provider(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Path(name): Path<String>,
) -> StatusCode {
    if db::providers::delete_provider_by_name(&state.db, &auth.user_id, &name).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
