use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use claude_chat_backend::{
    api, auth::middleware::AppState, config::Config, db,
    docker::{manager::DockerManager, registry::ContainerRegistry},
    ws::WsState,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

/// Build a test Config with required fields.
fn test_config() -> Config {
    Config {
        database_url: "sqlite::memory:".into(),
        jwt_secret: "test-jwt-secret-that-is-long-enough-for-hmac".into(),
        encryption_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        host: "127.0.0.1".into(),
        port: 0,
        container_image: "test:latest".into(),
        container_idle_timeout_secs: 600,
        internal_ws_port: 0,
        docker_network: None,
        host_data_dir: None,
        fileserver_url: None,
        cors_allowed_origins: None,
        access_token_ttl_secs: 7200,
        container_token_ttl_secs: 3600,
        refresh_token_ttl_days: 30,
    }
}

/// Build a test AppState with an in-memory SQLite DB.
async fn test_state() -> Arc<AppState> {
    let config = test_config();
    let pool = db::init_db("sqlite::memory:").await;
    let ws_state = WsState::new();
    let registry = ContainerRegistry::new();
    let docker_manager = Arc::new(DockerManager::new(config.clone(), registry));
    Arc::new(AppState {
        db: pool,
        config,
        ws_state,
        docker_manager,
    })
}

/// Build the auth router with test state.
fn auth_app(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/auth", api::auth::router())
        .with_state(state)
}

/// Helper to parse JSON response body.
async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Build a POST request with JSON body and required headers for rate limiter.
fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Build a GET request with auth token.
fn get_with_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

// ── Register ──

#[tokio::test]
async fn register_success() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"alice","email":"alice@example.com","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["email"], "alice@example.com");
}

#[tokio::test]
async fn register_short_username_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"ab","email":"ab@example.com","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_short_password_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"alice","email":"alice@example.com","password":"short"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_invalid_email_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"alice","email":"not-an-email","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_duplicate_username_rejected() {
    let state = test_state().await;

    // Register first user
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"alice","email":"alice@example.com","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Try duplicate username
    let app = auth_app(state);
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"alice","email":"alice2@example.com","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

// ── Login ──

#[tokio::test]
async fn login_success() {
    let state = test_state().await;

    // Register
    let app = auth_app(state.clone());
    app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"bob","email":"bob@example.com","password":"password123"}"#,
    )).await.unwrap();

    // Login
    let app = auth_app(state);
    let resp = app.oneshot(post_json(
        "/api/auth/login",
        r#"{"username":"bob","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert!(body["access_token"].is_string());
    assert_eq!(body["user"]["username"], "bob");
}

#[tokio::test]
async fn login_wrong_password() {
    let state = test_state().await;

    // Register
    let app = auth_app(state.clone());
    app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"carol","email":"carol@example.com","password":"password123"}"#,
    )).await.unwrap();

    // Login with wrong password
    let app = auth_app(state);
    let resp = app.oneshot(post_json(
        "/api/auth/login",
        r#"{"username":"carol","password":"wrongpassword"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_nonexistent_user() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app.oneshot(post_json(
        "/api/auth/login",
        r#"{"username":"nobody","password":"password123"}"#,
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Refresh ──

#[tokio::test]
async fn refresh_token_rotation() {
    let state = test_state().await;

    // Register and get tokens
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"dave","email":"dave@example.com","password":"password123"}"#,
    )).await.unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Use refresh token
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    let new_refresh_token = body["refresh_token"].as_str().unwrap().to_string();
    // New token should be different (rotation)
    assert_ne!(new_refresh_token, refresh_token);

    // Old token should no longer work
    let app = auth_app(state);
    let resp = app.oneshot(post_json(
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Logout ──

#[tokio::test]
async fn logout_invalidates_refresh_token() {
    let state = test_state().await;

    // Register
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"eve","email":"eve@example.com","password":"password123"}"#,
    )).await.unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Logout
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/logout",
        &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Refresh should fail after logout
    let app = auth_app(state);
    let resp = app.oneshot(post_json(
        "/api/auth/refresh",
        &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Auth Middleware ──

#[tokio::test]
async fn authenticated_endpoint_without_token_returns_401() {
    let state = test_state().await;
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/conversations")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authenticated_endpoint_with_valid_token_succeeds() {
    let state = test_state().await;

    // Register to get a token
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"frank","email":"frank@example.com","password":"password123"}"#,
    )).await.unwrap();
    let body = json_body(resp).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();

    // Use token to access protected endpoint
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);
    let resp = app.oneshot(get_with_auth("/api/conversations", &access_token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn authenticated_endpoint_with_invalid_token_returns_401() {
    let state = test_state().await;
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let resp = app.oneshot(get_with_auth("/api/conversations", "invalid-token-here")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_auth_scheme_returns_401() {
    let state = test_state().await;
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/conversations")
        .header("authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Query Param Token Fallback ──

#[tokio::test]
async fn query_param_token_authenticates_successfully() {
    let state = test_state().await;

    // Register to get a token
    let app = auth_app(state.clone());
    let resp = app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"grace","email":"grace@example.com","password":"password123"}"#,
    )).await.unwrap();
    let body = json_body(resp).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();

    // Use token via query param (no Authorization header)
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);
    let uri = format!("/api/conversations?token={}", access_token);
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn query_param_invalid_token_returns_401() {
    let state = test_state().await;
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/conversations?token=invalid-token")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
