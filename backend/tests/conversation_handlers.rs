use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use claude_chat_backend::{
    api,
    auth::middleware::AppState,
    config::Config,
    db,
    docker::{manager::DockerManager, registry::ContainerRegistry},
    ws::WsState,
};
use http_body_util::BodyExt;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use tower::ServiceExt;

fn serialize_models(models: &[&str]) -> String {
    serde_json::to_string(models).unwrap()
}

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
        cookie_secure: false,
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

fn get_with_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn delete_with_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn workspace_dir_for(conversation_id: &str) -> PathBuf {
    PathBuf::from(format!("data/conversations/{conversation_id}"))
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

fn token_user_id(state: &Arc<AppState>, token: &str) -> String {
    claude_chat_backend::auth::verify_access_token(token, &state.config.jwt_secret)
        .unwrap()
        .sub
}

async fn seed_provider(
    state: &Arc<AppState>,
    user_id: &str,
    id: &str,
    provider_type: &str,
    models: &[&str],
    image_models: &[&str],
) {
    let encrypted =
        claude_chat_backend::crypto::encrypt("seed-key", &state.config.encryption_key).unwrap();
    let models_json = serialize_models(models);
    let image_models_json = serialize_models(image_models);
    let first_model = models.first().copied();
    db::providers::upsert_provider(
        &state.db,
        Some(id),
        user_id,
        provider_type,
        &encrypted,
        None,
        first_model,
        false,
        Some(models_json.as_str()),
        Some(id),
        Some(image_models_json.as_str()),
    )
    .await
    .unwrap();
}

async fn seed_standard_providers(state: &Arc<AppState>, token: &str) {
    let user_id = token_user_id(state, token);
    seed_provider(
        state,
        &user_id,
        "openai",
        "openai",
        &["gpt-4o", "gpt-4.1-mini", "gpt-5.3-codex"],
        &[],
    )
    .await;
    seed_provider(
        state,
        &user_id,
        "anthropic",
        "anthropic",
        &["claude-3"],
        &[],
    )
    .await;
    seed_provider(
        state,
        &user_id,
        "My Google",
        "google",
        &["gemini-2.5-pro"],
        &["gemini-img", "gemini-img-v2"],
    )
    .await;
}

/// Create a conversation and return its id.
async fn create_conv(state: &Arc<AppState>, token: &str, provider_id: &str, model: &str) -> String {
    seed_standard_providers(state, token).await;
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(format!(
                    r#"{{"provider_id":"{}","model_name":"{}","subagent_provider_id":"{}","subagent_model":"{}"}}"#,
                    provider_id, model, provider_id, model
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
    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Switch provider_id from anthropic → openai
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"provider_id":"openai","model_name":"gpt-4o"}"#,
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

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Change model only (same provider_id)
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

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Update only the title — no provider_id/model change
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

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
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

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;

    // "Update" with the same provider_id and model — no actual change
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"provider_id":"openai","model_name":"gpt-4o"}"#,
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

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Set image_provider_id — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider_id":"My Google","image_model":"gemini-img"}"#,
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

    // First set an image provider_id
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider_id":"My Google","image_model":"gemini-img"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Re-add container connection
    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    // Change only image_model — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider_id":"My Google","image_model":"gemini-img-v2"}"#,
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

    // Set image provider_id first
    app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider_id":"My Google","image_model":"gemini-img"}"#,
            &token,
        ))
        .await
        .unwrap();

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;

    // Clear image provider_id — should restart container
    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"image_provider_id":"","image_model":""}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn create_conversation_requires_explicit_subagent_model() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o"}"#.to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("subagent_provider_id")
    );
}

#[tokio::test]
async fn create_conversation_rejects_unknown_provider_id() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"missing-provider","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o"}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("missing-provider")
    );
}

#[tokio::test]
async fn create_conversation_subagent_budget_defaults_to_main_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o","thinking_budget":200000}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = json_body(resp).await;
    assert_eq!(body["thinking_budget"], 200000);
    assert_eq!(body["subagent_thinking_budget"], 200000);
}

#[tokio::test]
async fn update_conversation_rejects_clearing_main_model_selection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"provider_id":"","model_name":""}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("provider_id")
    );
}

#[tokio::test]
async fn create_conversation_rejects_invalid_thinking_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o","thinking_budget":1000}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("thinking_budget")
    );
}

#[tokio::test]
async fn create_conversation_rejects_negative_subagent_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o","subagent_thinking_budget":-1}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("subagent_thinking_budget")
    );
}

#[tokio::test]
async fn create_conversation_rejects_budget_above_max() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o","thinking_budget":1000001}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("thinking_budget")
    );
}

#[tokio::test]
async fn create_conversation_accepts_budget_boundaries() {
    let state = test_state().await;
    let token = register_user(&state).await;
    seed_standard_providers(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider_id":"openai","model_name":"gpt-4o","subagent_provider_id":"openai","subagent_model":"gpt-4o","thinking_budget":1024,"subagent_thinking_budget":1000000}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = json_body(resp).await;
    assert_eq!(body["thinking_budget"], 1024);
    assert_eq!(body["subagent_thinking_budget"], 1000000);
}

#[tokio::test]
async fn update_subagent_budget_does_not_change_main_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"thinking_budget":180000,"subagent_thinking_budget":90000}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/conversations/{}", conv_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["thinking_budget"], 180000);
    assert_eq!(body["subagent_thinking_budget"], 90000);
}

#[tokio::test]
async fn update_conversation_rejects_invalid_subagent_budget_and_keeps_existing_values() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let update_resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"subagent_thinking_budget":100}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
    let update_body = json_body(update_resp).await;
    assert!(
        update_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("subagent_thinking_budget")
    );

    let get_resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = json_body(get_resp).await;
    assert_eq!(body["thinking_budget"], 128000);
    assert_eq!(body["subagent_thinking_budget"], 128000);
}

#[tokio::test]
async fn update_conversation_rejects_budget_above_max_and_keeps_existing_values() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let update_resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"thinking_budget":1000001}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
    let update_body = json_body(update_resp).await;
    assert!(
        update_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("thinking_budget")
    );

    let get_resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = json_body(get_resp).await;
    assert_eq!(body["thinking_budget"], 128000);
    assert_eq!(body["subagent_thinking_budget"], 128000);
}

#[tokio::test]
async fn update_conversation_rejects_negative_thinking_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let update_resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"thinking_budget":-1}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
    let update_body = json_body(update_resp).await;
    assert!(
        update_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("thinking_budget")
    );
}

#[tokio::test]
async fn update_conversation_rejects_both_invalid_budget_fields() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let update_resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"thinking_budget":0,"subagent_thinking_budget":1000001}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
    let update_body = json_body(update_resp).await;
    // Validation checks thinking_budget first.
    assert!(
        update_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("thinking_budget")
    );
}

#[tokio::test]
async fn update_subagent_model_only_removes_container_connection() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let (tx, _rx) = mpsc::channel(claude_chat_backend::ws::WS_CHANNEL_CAPACITY);
    state.ws_state.add_container(&conv_id, tx).await;
    assert!(state.ws_state.send_to_container(&conv_id, "ping").await);

    let resp = app(state.clone())
        .oneshot(put_json(
            &format!("/api/conversations/{}", conv_id),
            r#"{"subagent_model":"gpt-4.1-mini"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(!state.ws_state.send_to_container(&conv_id, "ping").await);
}

#[tokio::test]
async fn delete_conversation_removes_workspace_and_record() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;
    let workspace_dir = workspace_dir_for(&conv_id);
    let workspace_file = workspace_dir.join("notes.txt");

    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();
    tokio::fs::write(&workspace_file, b"hello").await.unwrap();

    let resp = app(state.clone())
        .oneshot(delete_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(!workspace_dir.exists());

    let get_resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_conversation_keeps_record_when_workspace_delete_fails() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;
    let workspace_dir = workspace_dir_for(&conv_id);

    if workspace_dir.exists() {
        tokio::fs::remove_dir_all(&workspace_dir).await.unwrap();
    }
    if let Some(parent) = workspace_dir.parent() {
        tokio::fs::create_dir_all(parent).await.unwrap();
    }
    tokio::fs::write(&workspace_dir, b"not a directory")
        .await
        .unwrap();

    let resp = app(state.clone())
        .oneshot(delete_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let get_resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);

    if workspace_dir.exists() {
        tokio::fs::remove_file(&workspace_dir).await.unwrap();
    }
}

#[tokio::test]
async fn delete_conversation_succeeds_when_workspace_missing() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;
    let workspace_dir = workspace_dir_for(&conv_id);

    if workspace_dir.exists() {
        tokio::fs::remove_dir_all(&workspace_dir).await.unwrap();
    }

    let resp = app(state.clone())
        .oneshot(delete_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let get_resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_messages_returns_structured_parts_when_available() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let legacy = db::messages::create_message(
        &state.db,
        &conv_id,
        "assistant",
        "legacy content",
        None,
        None,
        Some(7),
    )
    .await
    .unwrap();

    let _ = db::messages_v2::create_message_with_parts(
        &state.db,
        Some(legacy.id.as_str()),
        &conv_id,
        "assistant",
        Some("openai"),
        Some("gpt-4o"),
        Some(r#"{"completion":7}"#),
        None,
        &[
            db::messages_v2::NewMessagePart {
                part_type: "reasoning",
                text: Some("chain-of-thought summary"),
                json_payload: Some(r#"{"raw":"r"}"#),
                tool_call_id: None,
            },
            db::messages_v2::NewMessagePart {
                part_type: "text",
                text: Some("final answer"),
                json_payload: None,
                tool_call_id: None,
            },
        ],
    )
    .await
    .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}/messages?limit=10&offset=0", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    let parts = msgs[0]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["type"], "reasoning");
    assert_eq!(parts[1]["type"], "text");
    assert_eq!(parts[1]["text"], "final answer");
}

#[tokio::test]
async fn list_messages_synthesizes_text_part_for_legacy_rows() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let _legacy = db::messages::create_message(
        &state.db,
        &conv_id,
        "user",
        "hello from legacy",
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}/messages?limit=10&offset=0", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    let parts = msgs[0]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[0]["text"], "hello from legacy");
}

#[tokio::test]
async fn list_messages_synthesizes_tool_result_part_for_legacy_tool_calls() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token, "openai", "gpt-4o").await;

    let tool_blocks = serde_json::json!([
        {"type":"text","content":"running"},
        {
            "type":"tool_call",
            "id":"tc-legacy-1",
            "name":"bash",
            "input":{"command":"ls"},
            "result":{"kind":"bash","text":"file-a\nfile-b"}
        }
    ]);
    let _legacy = db::messages::create_message(
        &state.db,
        &conv_id,
        "assistant",
        "final",
        Some(&tool_blocks.to_string()),
        None,
        None,
    )
    .await
    .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth(
            &format!("/api/conversations/{}/messages?limit=10&offset=0", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    let parts = msgs[0]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[1]["type"], "tool_call");
    assert_eq!(parts[2]["type"], "tool_result");
    assert_eq!(parts[2]["tool_call_id"], "tc-legacy-1");
    assert_eq!(parts[2]["text"], "file-a\nfile-b");
}

#[tokio::test]
async fn list_conversations_uses_stable_tiebreakers_when_updated_at_matches() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_old = create_conv(&state, &token, "openai", "gpt-4o").await;
    let conv_new = create_conv(&state, &token, "openai", "gpt-4o").await;

    // Force same updated_at for both rows and different created_at values.
    sqlx::query("UPDATE conversations SET updated_at = ?, created_at = ? WHERE id = ?")
        .bind("2000-01-01 00:00:00")
        .bind("1999-01-01 00:00:00")
        .bind(&conv_old)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE conversations SET updated_at = ?, created_at = ? WHERE id = ?")
        .bind("2000-01-01 00:00:00")
        .bind("2001-01-01 00:00:00")
        .bind(&conv_new)
        .execute(&state.db)
        .await
        .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth("/api/conversations", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    let convs = body.as_array().unwrap();
    assert_eq!(convs.len(), 2);
    assert_eq!(convs[0]["id"], conv_new);
    assert_eq!(convs[1]["id"], conv_old);
}
