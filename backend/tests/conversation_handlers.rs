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

#[tokio::test]
async fn create_conversation_defaults_subagent_to_main_model() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o"}"#.to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = json_body(resp).await;
    assert_eq!(body["provider"], "openai");
    assert_eq!(body["model_name"], "gpt-4o");
    assert_eq!(body["subagent_provider"], "openai");
    assert_eq!(body["subagent_model"], "gpt-4o");
    assert_eq!(body["thinking_budget"], 128000);
    assert_eq!(body["subagent_thinking_budget"], 128000);
}

#[tokio::test]
async fn create_conversation_subagent_budget_defaults_to_main_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o","thinking_budget":200000}"#
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
async fn create_conversation_rejects_invalid_thinking_budget() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o","thinking_budget":1000}"#
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

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o","subagent_thinking_budget":-1}"#
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

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o","thinking_budget":1000001}"#
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

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    r#"{"provider":"openai","model_name":"gpt-4o","thinking_budget":1024,"subagent_thinking_budget":1000000}"#
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

    let (tx, _rx) = mpsc::unbounded_channel();
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
