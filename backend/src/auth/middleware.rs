use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Extension;

use crate::config::Config;

/// Shared application state, stored as `Extension<Arc<AppState>>` on the router.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: Config,
}

/// Extractor that authenticates a request via the `Authorization: Bearer <token>`
/// header and provides the caller's identity.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Retrieve AppState from request extensions.
        let Extension(state): Extension<Arc<AppState>> =
            Extension::from_request_parts(parts, _state)
                .await
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "application state not configured",
                    )
                })?;

        // Extract the Bearer token from the Authorization header.
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization header"))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "invalid authorization scheme"))?;

        let claims =
            super::verify_access_token(token, &state.config.jwt_secret)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid or expired token"))?;

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

impl<S> FromRequestParts<S> for AdminOnly
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err((StatusCode::FORBIDDEN, "admin privileges required"));
        }
        Ok(AdminOnly(user))
    }
}
