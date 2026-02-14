use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::config::Config;
use crate::docker::manager::DockerManager;
use crate::error::AppError;
use crate::ws::WsState;

/// Shared application state, stored as `Router::with_state(Arc<AppState>)`.
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: Config,
    pub ws_state: Arc<WsState>,
    pub docker_manager: Arc<DockerManager>,
}

/// Extractor that authenticates a request via the `Authorization: Bearer <token>`
/// header and provides the caller's identity.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // 1. Try Authorization header
        // 2. Fallback to ?token= query param (for <img>/<video>/<audio> src requests)
        let token = if let Some(auth_header) = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
        {
            auth_header
                .strip_prefix("Bearer ")
                .ok_or_else(|| AppError::Unauthorized("Invalid authorization scheme".into()))?
                .to_owned()
        } else if let Some(query) = parts.uri.query() {
            form_urlencoded::parse(query.as_bytes())
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.into_owned())
                .ok_or_else(|| AppError::Unauthorized("Missing authorization".into()))?
        } else {
            return Err(AppError::Unauthorized("Missing authorization".into()));
        };

        let claims =
            super::verify_access_token(&token, &state.config.jwt_secret)
                .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
            is_admin: claims.is_admin,
        })
    }
}

/// Extractor that requires the caller to be an authenticated **admin** user.
///
/// If the user is authenticated but not an admin, a `403 Forbidden` is returned.
#[derive(Debug, Clone)]
pub struct AdminOnly(pub AuthUser);

impl FromRequestParts<Arc<AppState>> for AdminOnly {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(AppError::Forbidden("Admin privileges required".into()));
        }
        Ok(AdminOnly(user))
    }
}
