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
        .route("/me/providers/{provider}", delete(delete_provider))
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
    pub provider: String,
    pub endpoint_url: Option<String>,
    pub model_name: Option<String>,
    pub is_default: bool,
    pub has_api_key: bool,
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
                provider: p.provider,
                endpoint_url: p.endpoint_url,
                model_name: p.model_name,
                is_default: p.is_default,
                has_api_key: true,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct UpsertProviderRequest {
    pub provider: String,
    pub api_key: String,
    pub endpoint_url: Option<String>,
    pub model_name: Option<String>,
    pub is_default: bool,
}

async fn upsert_provider(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpsertProviderRequest>,
) -> Result<Json<ProviderResponse>, (StatusCode, String)> {
    let valid_providers = ["openai", "anthropic", "google", "mistral"];
    if !valid_providers.contains(&req.provider.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid provider".into()));
    }

    let encrypted_key = crypto::encrypt(&req.api_key, &state.config.encryption_key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // If setting as default, unset other defaults
    if req.is_default {
        let providers = db::providers::list_providers(&state.db, &auth.user_id).await;
        for p in providers {
            if p.is_default && p.provider != req.provider {
                db::providers::upsert_provider(
                    &state.db,
                    Some(&p.id),
                    &auth.user_id,
                    &p.provider,
                    &p.api_key_encrypted,
                    p.endpoint_url.as_deref(),
                    p.model_name.as_deref(),
                    false,
                ).await;
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let provider = db::providers::upsert_provider(
        &state.db,
        Some(&id),
        &auth.user_id,
        &req.provider,
        &encrypted_key,
        req.endpoint_url.as_deref(),
        req.model_name.as_deref(),
        req.is_default,
    ).await;

    Ok(Json(ProviderResponse {
        id: provider.id,
        provider: provider.provider,
        endpoint_url: provider.endpoint_url,
        model_name: provider.model_name,
        is_default: provider.is_default,
        has_api_key: true,
    }))
}

async fn delete_provider(
    Extension(state): Extension<Arc<AppState>>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> StatusCode {
    if db::providers::delete_provider(&state.db, &auth.user_id, &provider).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
