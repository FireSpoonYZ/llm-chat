use axum::{
    extract::Extension,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::auth;
use crate::auth::middleware::AppState;
use crate::auth::password;
use crate::db;

pub fn router() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserResponse,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

async fn register(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<MessageResponse>)> {
    if req.username.len() < 3 || req.username.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(MessageResponse { message: "Username must be 3-50 characters".into() }),
        ));
    }
    if req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(MessageResponse { message: "Password must be at least 8 characters".into() }),
        ));
    }

    if db::users::get_user_by_username(&state.db, &req.username).await.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(MessageResponse { message: "Username already taken".into() }),
        ));
    }
    if db::users::get_user_by_email(&state.db, &req.email).await.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(MessageResponse { message: "Email already registered".into() }),
        ));
    }

    let password_hash = password::hash_password(&req.password).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: e }))
    })?;

    let user = db::users::create_user(&state.db, &req.username, &req.email, &password_hash).await;

    let access_token = auth::create_access_token(&user.id, &user.username, user.is_admin, &state.config.jwt_secret)
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: e.to_string() }))
        })?;

    let (refresh_token, token_hash) = generate_refresh_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
    db::refresh_tokens::create_refresh_token(&state.db, &user.id, &token_hash, &expires_at.to_rfc3339()).await;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        user: UserResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            is_admin: user.is_admin,
        },
    }))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

async fn login(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<MessageResponse>)> {
    let user = db::users::get_user_by_username(&state.db, &req.username).await
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(MessageResponse { message: "Invalid credentials".into() }))
        })?;

    let valid = password::verify_password(&req.password, &user.password_hash).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: e }))
    })?;

    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(MessageResponse { message: "Invalid credentials".into() }),
        ));
    }

    let access_token = auth::create_access_token(&user.id, &user.username, user.is_admin, &state.config.jwt_secret)
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: e.to_string() }))
        })?;

    let (refresh_token, token_hash) = generate_refresh_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
    db::refresh_tokens::create_refresh_token(&state.db, &user.id, &token_hash, &expires_at.to_rfc3339()).await;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        user: UserResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            is_admin: user.is_admin,
        },
    }))
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

async fn refresh(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<MessageResponse>)> {
    let token_hash = hash_token(&req.refresh_token);

    let stored = db::refresh_tokens::get_refresh_token_by_hash(&state.db, &token_hash).await
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(MessageResponse { message: "Invalid refresh token".into() }))
        })?;

    let expires_at = chrono::DateTime::parse_from_rfc3339(&stored.expires_at)
        .map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: "Invalid token expiry".into() }))
        })?;

    if expires_at < chrono::Utc::now() {
        db::refresh_tokens::delete_refresh_token(&state.db, &stored.id).await;
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(MessageResponse { message: "Refresh token expired".into() }),
        ));
    }

    // Delete old token (rotation)
    db::refresh_tokens::delete_refresh_token(&state.db, &stored.id).await;

    let user = db::users::get_user_by_id(&state.db, &stored.user_id).await
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(MessageResponse { message: "User not found".into() }))
        })?;

    let access_token = auth::create_access_token(&user.id, &user.username, user.is_admin, &state.config.jwt_secret)
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(MessageResponse { message: e.to_string() }))
        })?;

    let (new_refresh_token, new_token_hash) = generate_refresh_token();
    let new_expires_at = chrono::Utc::now() + chrono::Duration::days(30);
    db::refresh_tokens::create_refresh_token(&state.db, &user.id, &new_token_hash, &new_expires_at.to_rfc3339()).await;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token: new_refresh_token,
        user: UserResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            is_admin: user.is_admin,
        },
    }))
}

async fn logout(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Json<MessageResponse> {
    let token_hash = hash_token(&req.refresh_token);
    db::refresh_tokens::delete_refresh_token_by_hash(&state.db, &token_hash).await;
    Json(MessageResponse { message: "Logged out".into() })
}

fn generate_refresh_token() -> (String, String) {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
