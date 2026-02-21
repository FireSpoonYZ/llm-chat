use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use validator::Validate;

use crate::auth;
use crate::auth::middleware::AppState;
use crate::auth::password;
use crate::db;
use crate::error::AppError;

pub fn router() -> Router<Arc<AppState>> {
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(6)
        .burst_size(10)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .unwrap();

    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .layer(GovernorLayer::new(governor_conf))
}

#[derive(Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 50, message = "Username must be 3-50 characters"))]
    pub username: String,
    #[validate(email(message = "Invalid email address"))]
    pub email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
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

#[derive(sqlx::FromRow)]
struct ConsumedRefreshToken {
    user_id: String,
    expires_at: String,
}

fn append_set_cookie(headers: &mut HeaderMap, cookie: String) -> Result<(), AppError> {
    let value = HeaderValue::from_str(&cookie).map_err(|e| AppError::Internal(e.to_string()))?;
    headers.append(header::SET_COOKIE, value);
    Ok(())
}

fn refresh_ttl_secs(refresh_token_ttl_days: i64) -> i64 {
    refresh_token_ttl_days.saturating_mul(24 * 60 * 60)
}

fn build_cookie(
    name: &str,
    value: &str,
    max_age_secs: i64,
    secure: bool,
    http_only: bool,
) -> String {
    let mut cookie = format!("{name}={value}; Path=/; Max-Age={max_age_secs}; SameSite=Lax");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn set_auth_cookies(
    headers: &mut HeaderMap,
    access_token: &str,
    refresh_token: &str,
    state: &AppState,
) -> Result<(), AppError> {
    append_set_cookie(
        headers,
        build_cookie(
            auth::ACCESS_COOKIE_NAME,
            access_token,
            state.config.access_token_ttl_secs as i64,
            state.config.cookie_secure,
            true,
        ),
    )?;
    append_set_cookie(
        headers,
        build_cookie(
            auth::REFRESH_COOKIE_NAME,
            refresh_token,
            refresh_ttl_secs(state.config.refresh_token_ttl_days),
            state.config.cookie_secure,
            true,
        ),
    )?;
    Ok(())
}

fn clear_auth_cookies(headers: &mut HeaderMap, state: &AppState) -> Result<(), AppError> {
    append_set_cookie(
        headers,
        build_cookie(
            auth::ACCESS_COOKIE_NAME,
            "",
            0,
            state.config.cookie_secure,
            true,
        ),
    )?;
    append_set_cookie(
        headers,
        build_cookie(
            auth::REFRESH_COOKIE_NAME,
            "",
            0,
            state.config.cookie_secure,
            true,
        ),
    )?;
    Ok(())
}

fn auth_response(
    user: &db::users::User,
    access_token: String,
    refresh_token: String,
) -> AuthResponse {
    AuthResponse {
        access_token,
        refresh_token,
        user: UserResponse {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            is_admin: user.is_admin,
        },
    }
}

fn map_user_create_error(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.is_unique_violation()
    {
        let msg = db_err.message();
        if msg.contains("users.username") {
            return AppError::Conflict("Username already taken".into());
        }
        if msg.contains("users.email") {
            return AppError::Conflict("Email already registered".into());
        }
        return AppError::Conflict("User already exists".into());
    }
    AppError::from(err)
}

async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if db::users::get_user_by_username(&state.db, &req.username)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Username already taken".into()));
    }
    if db::users::get_user_by_email(&state.db, &req.email)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    let pw = req.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || password::hash_password(&pw))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(AppError::from)?;

    let mut tx = state.db.begin().await?;
    let user = db::users::create_user_in_tx(&mut tx, &req.username, &req.email, &password_hash)
        .await
        .map_err(map_user_create_error)?;
    db::presets::ensure_builtin_presets_for_user_in_tx(&mut tx, &user.id).await?;

    let access_token = auth::create_access_token(
        &user.id,
        &user.username,
        user.is_admin,
        &state.config.jwt_secret,
        state.config.access_token_ttl_secs,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let (refresh_token, token_hash) = generate_refresh_token();
    let expires_at =
        chrono::Utc::now() + chrono::Duration::days(state.config.refresh_token_ttl_days);
    db::refresh_tokens::create_refresh_token_in_tx(
        &mut tx,
        &user.id,
        &token_hash,
        &expires_at.to_rfc3339(),
    )
    .await?;
    tx.commit().await?;

    let mut response = Json(auth_response(
        &user,
        access_token.clone(),
        refresh_token.clone(),
    ))
    .into_response();
    set_auth_cookies(
        response.headers_mut(),
        &access_token,
        &refresh_token,
        &state,
    )?;
    Ok(response)
}

#[derive(Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 1, message = "Username is required"))]
    pub username: String,
    #[validate(length(min = 1, message = "Password is required"))]
    pub password: String,
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let user = db::users::get_user_by_username(&state.db, &req.username)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid credentials".into()))?;

    let pw = req.password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || password::verify_password(&pw, &hash))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(AppError::from)?;

    if !valid {
        return Err(AppError::Unauthorized("Invalid credentials".into()));
    }

    let access_token = auth::create_access_token(
        &user.id,
        &user.username,
        user.is_admin,
        &state.config.jwt_secret,
        state.config.access_token_ttl_secs,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let (refresh_token, token_hash) = generate_refresh_token();
    let expires_at =
        chrono::Utc::now() + chrono::Duration::days(state.config.refresh_token_ttl_days);
    db::refresh_tokens::create_refresh_token(
        &state.db,
        &user.id,
        &token_hash,
        &expires_at.to_rfc3339(),
    )
    .await?;

    let mut response = Json(auth_response(
        &user,
        access_token.clone(),
        refresh_token.clone(),
    ))
    .into_response();
    set_auth_cookies(
        response.headers_mut(),
        &access_token,
        &refresh_token,
        &state,
    )?;
    Ok(response)
}

#[derive(Deserialize, Default)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

fn extract_refresh_token(headers: &HeaderMap, req: &RefreshRequest) -> Option<String> {
    req.refresh_token
        .clone()
        .or_else(|| auth::get_cookie(headers, auth::REFRESH_COOKIE_NAME))
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RefreshRequest>,
) -> Result<Response, AppError> {
    let refresh_token = extract_refresh_token(&headers, &req)
        .ok_or_else(|| AppError::Unauthorized("Missing refresh token".into()))?;
    let token_hash = hash_token(&refresh_token);
    let now = chrono::Utc::now();
    let mut tx = state.db.begin().await?;

    // Consume token in the same transaction to prevent refresh-token replay.
    let consumed = sqlx::query_as::<_, ConsumedRefreshToken>(
        "DELETE FROM refresh_tokens
         WHERE token_hash = ?
         RETURNING user_id, expires_at",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid refresh token".into()))?;

    let expires_at = chrono::DateTime::parse_from_rfc3339(&consumed.expires_at)
        .map_err(|_| AppError::Internal("Invalid token expiry".into()))?;

    if expires_at < now {
        tx.commit().await?;
        return Err(AppError::Unauthorized("Refresh token expired".into()));
    }

    let user = sqlx::query_as::<_, db::users::User>(
        "SELECT id, username, email, password_hash, is_admin, created_at, updated_at
         FROM users
         WHERE id = ?",
    )
    .bind(&consumed.user_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    let access_token = auth::create_access_token(
        &user.id,
        &user.username,
        user.is_admin,
        &state.config.jwt_secret,
        state.config.access_token_ttl_secs,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let (new_refresh_token, new_token_hash) = generate_refresh_token();
    let new_expires_at = now + chrono::Duration::days(state.config.refresh_token_ttl_days);
    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&user.id)
    .bind(&new_token_hash)
    .bind(new_expires_at.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut response = Json(auth_response(
        &user,
        access_token.clone(),
        new_refresh_token.clone(),
    ))
    .into_response();
    set_auth_cookies(
        response.headers_mut(),
        &access_token,
        &new_refresh_token,
        &state,
    )?;
    Ok(response)
}

#[derive(Deserialize, Default)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LogoutRequest>,
) -> Result<Response, AppError> {
    let refresh_token = req
        .refresh_token
        .or_else(|| auth::get_cookie(&headers, auth::REFRESH_COOKIE_NAME));

    if let Some(token) = refresh_token {
        let token_hash = hash_token(&token);
        db::refresh_tokens::delete_refresh_token_by_hash(&state.db, &token_hash).await?;
    }

    let mut response = Json(MessageResponse {
        message: "Logged out".into(),
    })
    .into_response();
    clear_auth_cookies(response.headers_mut(), &state)?;
    Ok(response)
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
