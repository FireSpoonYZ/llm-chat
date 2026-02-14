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
use tokio::sync::mpsc;
use tower::ServiceExt;

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

fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/auth", api::auth::router())
        .nest("/api/conversations", api::conversations::router())
        .with_state(state)
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn put_json(uri: &str, body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Register a user and return the access token.
async fn register_user(state: &Arc<AppState>) -> String {
    let resp = app(state.clone())
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"testuser","email":"test@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    body["access_token"].as_str().unwrap().to_string()
}

/// Create a conversation and return its id.
async fn create_conv(state: &Arc<AppState>, token: &str, provider: &str, model: &str) -> String {
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(format!(
                    r#"{{"provider":"{}","model_name":"{}"}}"#,
                    provider, model
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = json_body(resp).await;
    body["id"].as_str().unwrap().to_string()
}

// ── Tests ──

#[tokio::test]
async fn update_provider_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "anthropic", "claude-3").await;

    // Simulate a running container by adding a WS connection
    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Switch provider from anthropic → openai
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"provider":"openai","model_name":"gpt-4o"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container connection should have been removed
    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_model_only_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Change model only (same provider)
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"model_name":"gpt-5.3-codex"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container should be removed because model changed
    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_title_keeps_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Update only the title — no provider/model change
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"title":"Renamed chat"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container should still be connected
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_system_prompt_keeps_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;

    // Update only system prompt
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"system_prompt_override":"You are a helpful assistant."}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container should still be connected
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_same_provider_keeps_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;

    // "Update" with the same provider and model — no actual change
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"provider":"openai","model_name":"gpt-4o"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container should still be connected (nothing changed)
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_image_provider_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Set image_provider — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider":"My Google","image_model":"gemini-img"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Container connection should have been removed
    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn update_image_model_only_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    // First set an image provider
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider":"My Google","image_model":"gemini-img"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Re-add container connection
    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Change only image_model — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider":"My Google","image_model":"gemini-img-v2"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn clear_image_provider_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    // Set image provider first
    app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider":"My Google","image_model":"gemini-img"}"#,
            &token,
        ))
        .await
        .unwrap();

    let (tx, _rx) = mpsc::unbounded_channel();
    state.ws_state.add_container(&conv_id, tx).await;

    // Clear image provider — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider":"","image_model":""}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}
