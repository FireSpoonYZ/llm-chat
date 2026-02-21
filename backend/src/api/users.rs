use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use validator::Validate;

use crate::auth::middleware::{AppState, AuthUser};
use crate::crypto;
use crate::db;
use crate::error::AppError;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/me", get(get_profile))
        .route("/me/providers", get(list_providers).post(upsert_provider))
        .route("/me/providers/{id}", delete(delete_provider))
        .route(
            "/me/model-defaults",
            get(get_model_defaults).put(update_model_defaults),
        )
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
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ProfileResponse>, AppError> {
    let user = db::users::get_user_by_id(&state.db, &auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

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
    pub image_models: Vec<String>,
    pub is_default: bool,
    pub has_api_key: bool,
}

fn parse_models_json(json_str: Option<&str>) -> Vec<String> {
    json_str
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

async fn list_providers(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<ProviderResponse>>, AppError> {
    let providers = db::providers::list_providers(&state.db, &auth.user_id).await?;
    Ok(Json(
        providers
            .into_iter()
            .map(|p| ProviderResponse {
                id: p.id,
                name: p.name.unwrap_or_else(|| p.provider.clone()),
                provider: p.provider,
                endpoint_url: p.endpoint_url,
                models: parse_models_json(p.models.as_deref()),
                image_models: parse_models_json(p.image_models.as_deref()),
                is_default: p.is_default,
                has_api_key: true,
            })
            .collect(),
    ))
}

fn validate_provider_type(provider_type: &str) -> Result<(), validator::ValidationError> {
    const VALID_PROVIDERS: &[&str] = &["openai", "anthropic", "google", "mistral"];
    if VALID_PROVIDERS.contains(&provider_type) {
        Ok(())
    } else {
        Err(validator::ValidationError::new("invalid_provider_type")
            .with_message("Invalid provider type".into()))
    }
}

#[derive(Deserialize, Validate)]
pub struct UpsertProviderRequest {
    pub id: Option<String>,
    #[validate(length(min = 1, message = "Name is required"))]
    pub name: String,
    #[validate(custom(function = "validate_provider_type"))]
    pub provider_type: String,
    pub api_key: String,
    pub endpoint_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub image_models: Option<Vec<String>>,
    pub is_default: Option<bool>,
}

async fn upsert_provider(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpsertProviderRequest>,
) -> Result<Json<ProviderResponse>, AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let has_chat_models = req.models.as_ref().is_some_and(|models| !models.is_empty());
    let has_image_models = req
        .image_models
        .as_ref()
        .is_some_and(|models| !models.is_empty());
    if !has_chat_models && !has_image_models {
        return Err(AppError::BadRequest(
            "Provider must include at least one chat model or one image model".into(),
        ));
    }

    let provider_id = normalize_optional_string(req.id.as_deref());
    let existing_provider = if let Some(id) = provider_id.as_deref() {
        db::providers::get_provider_by_id(&state.db, &auth.user_id, id).await?
    } else {
        None
    };

    if provider_id.is_some() && existing_provider.is_none() {
        return Err(AppError::BadRequest("Provider id does not exist".into()));
    }

    let is_default = req
        .is_default
        .unwrap_or_else(|| existing_provider.as_ref().is_some_and(|p| p.is_default));

    // If editing and keeping existing key, reuse encrypted key from the existing provider id.
    let encrypted_key = if req.api_key == "__KEEP_EXISTING__" {
        match existing_provider.as_ref() {
            Some(p) => p.api_key_encrypted.clone(),
            None => {
                return Err(AppError::BadRequest(
                    "API key is required for new providers".into(),
                ));
            }
        }
    } else {
        crypto::encrypt(&req.api_key, &state.config.encryption_key)?
    };

    let models_json = req
        .models
        .as_ref()
        .and_then(|m| serde_json::to_string(m).ok());
    let image_models_json = req
        .image_models
        .as_ref()
        .and_then(|m| serde_json::to_string(m).ok());
    let first_model = req.models.as_ref().and_then(|m| m.first().cloned());

    let provider = db::providers::upsert_provider(
        &state.db,
        provider_id.as_deref(),
        &auth.user_id,
        &req.provider_type,
        &encrypted_key,
        req.endpoint_url.as_deref(),
        first_model.as_deref(),
        is_default,
        models_json.as_deref(),
        Some(&req.name),
        image_models_json.as_deref(),
    )
    .await?;
    let _ = db::model_defaults::prune_invalid_provider_references(&state.db, &auth.user_id).await?;

    Ok(Json(ProviderResponse {
        id: provider.id,
        name: provider.name.unwrap_or_else(|| provider.provider.clone()),
        provider: provider.provider,
        endpoint_url: provider.endpoint_url,
        models: parse_models_json(provider.models.as_deref()),
        image_models: parse_models_json(provider.image_models.as_deref()),
        is_default: provider.is_default,
        has_api_key: true,
    }))
}

async fn delete_provider(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if db::providers::delete_provider_by_id(&state.db, &auth.user_id, &id).await? {
        let _ =
            db::model_defaults::clear_provider_references(&state.db, &auth.user_id, &id).await?;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

#[derive(Serialize)]
pub struct ModelDefaultsResponse {
    pub chat_provider_id: Option<String>,
    pub chat_model: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model: Option<String>,
    pub image_provider_id: Option<String>,
    pub image_model: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateModelDefaultsRequest {
    pub chat_provider_id: Option<String>,
    pub chat_model: Option<String>,
    pub subagent_provider_id: Option<String>,
    pub subagent_model: Option<String>,
    pub image_provider_id: Option<String>,
    pub image_model: Option<String>,
}

async fn get_model_defaults(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ModelDefaultsResponse>, AppError> {
    let defaults = db::model_defaults::get_or_init_model_defaults(&state.db, &auth.user_id).await?;
    Ok(Json(to_model_defaults_response(defaults)))
}

async fn update_model_defaults(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateModelDefaultsRequest>,
) -> Result<Json<ModelDefaultsResponse>, AppError> {
    let chat_provider_id = normalize_optional_string(req.chat_provider_id.as_deref())
        .ok_or_else(|| AppError::BadRequest("chat_provider_id is required".into()))?;
    let chat_model = normalize_optional_string(req.chat_model.as_deref())
        .ok_or_else(|| AppError::BadRequest("chat_model is required".into()))?;
    let subagent_provider_id = normalize_optional_string(req.subagent_provider_id.as_deref())
        .ok_or_else(|| AppError::BadRequest("subagent_provider_id is required".into()))?;
    let subagent_model = normalize_optional_string(req.subagent_model.as_deref())
        .ok_or_else(|| AppError::BadRequest("subagent_model is required".into()))?;
    let image_provider_id = normalize_optional_string(req.image_provider_id.as_deref());
    let image_model = normalize_optional_string(req.image_model.as_deref());

    if image_provider_id.is_some() ^ image_model.is_some() {
        return Err(AppError::BadRequest(
            "image_provider_id and image_model must both be set or both be empty".into(),
        ));
    }

    let providers = db::providers::list_providers(&state.db, &auth.user_id).await?;
    ensure_provider_has_model(&providers, &chat_provider_id, &chat_model, false)?;
    ensure_provider_has_model(&providers, &subagent_provider_id, &subagent_model, false)?;
    if let (Some(provider_id), Some(model)) = (image_provider_id.as_deref(), image_model.as_deref())
    {
        ensure_provider_has_model(&providers, provider_id, model, true)?;
    }

    let saved = db::model_defaults::upsert_model_defaults(
        &state.db,
        &auth.user_id,
        Some(chat_provider_id.as_str()),
        Some(chat_model.as_str()),
        Some(subagent_provider_id.as_str()),
        Some(subagent_model.as_str()),
        image_provider_id.as_deref(),
        image_model.as_deref(),
    )
    .await?;
    Ok(Json(to_model_defaults_response(saved)))
}

fn to_model_defaults_response(
    defaults: db::model_defaults::UserModelDefaults,
) -> ModelDefaultsResponse {
    ModelDefaultsResponse {
        chat_provider_id: defaults.chat_provider_id,
        chat_model: defaults.chat_model_name,
        subagent_provider_id: defaults.subagent_provider_id,
        subagent_model: defaults.subagent_model_name,
        image_provider_id: defaults.image_provider_id,
        image_model: defaults.image_model_name,
    }
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

fn ensure_provider_has_model(
    providers: &[db::providers::UserProvider],
    provider_id: &str,
    model_name: &str,
    use_image_models: bool,
) -> Result<(), AppError> {
    let provider = providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| {
            AppError::BadRequest(format!("Provider id '{provider_id}' does not exist"))
        })?;

    let models = if use_image_models {
        parse_models_json(provider.image_models.as_deref())
    } else {
        parse_models_json(provider.models.as_deref())
    };
    if models.iter().any(|m| m == model_name) {
        return Ok(());
    }
    let kind = if use_image_models {
        "image model"
    } else {
        "model"
    };
    Err(AppError::BadRequest(format!(
        "{kind} '{model_name}' is not available for provider id '{provider_id}'"
    )))
}
