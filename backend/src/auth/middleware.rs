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

/// Extractor that authenticates a request via either:
/// 1) `Authorization: Bearer <token>`
/// 2) `access_token` HttpOnly cookie.
///
/// and provides the caller's identity.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub is_admin: bool,
}

/// Route-scoped extractor that also accepts `?token=...` for media/file URLs.
#[derive(Debug, Clone)]
pub struct QueryAuthUser(pub AuthUser);

impl std::ops::Deref for QueryAuthUser {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn token_from_header_or_cookie(parts: &Parts) -> Result<Option<String>, AppError> {
    if let Some(auth_header) = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid authorization scheme".into()))?
            .to_owned();
        Ok(Some(token))
    } else {
        Ok(super::get_cookie(&parts.headers, super::ACCESS_COOKIE_NAME))
    }
}

fn token_from_query(parts: &Parts) -> Option<String> {
    parts.uri.query().and_then(|query| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == "token")
            .map(|(_, v)| v.into_owned())
    })
}

fn authenticate(
    parts: &Parts,
    state: &Arc<AppState>,
    allow_query_token: bool,
) -> Result<AuthUser, AppError> {
    let token = if let Some(token) = token_from_header_or_cookie(parts)? {
        token
    } else if allow_query_token {
        token_from_query(parts)
            .ok_or_else(|| AppError::Unauthorized("Missing authorization".into()))?
    } else {
        return Err(AppError::Unauthorized("Missing authorization".into()));
    };

    let claims = super::verify_access_token(&token, &state.config.jwt_secret)
        .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

    Ok(AuthUser {
        user_id: claims.sub,
        is_admin: claims.is_admin,
    })
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        authenticate(parts, state, false)
    }
}

impl FromRequestParts<Arc<AppState>> for QueryAuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(QueryAuthUser(authenticate(parts, state, true)?))
    }
}

/// Extractor that requires the caller to be an authenticated **admin** user.
///
/// If the user is authenticated but not an admin, a `403 Forbidden` is returned.
#[derive(Debug, Clone)]
pub struct AdminOnly;

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
        Ok(AdminOnly)
    }
}
