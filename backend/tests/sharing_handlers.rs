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
        .nest("/api/conversations", api::sharing::share_management_router())
        .nest("/api/shared", api::sharing::shared_router())
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

fn authed_post(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn authed_delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn unauthed_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

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

async fn register_user2(state: &Arc<AppState>) -> String {
    let resp = app(state.clone())
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"otheruser","email":"other@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    body["access_token"].as_str().unwrap().to_string()
}

async fn create_conv(state: &Arc<AppState>, token: &str) -> String {
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(r#"{"title":"Test Chat"}"#))
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
async fn create_share_returns_token() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert!(body["share_token"].as_str().unwrap().len() == 64); // 32 bytes hex
    assert!(body["share_url"].as_str().unwrap().starts_with("/share/"));
}

#[tokio::test]
async fn create_share_idempotent() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    let resp1 = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    let body1 = json_body(resp1).await;

    let resp2 = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    let body2 = json_body(resp2).await;

    assert_eq!(body1["share_token"], body2["share_token"]);
}

#[tokio::test]
async fn create_share_unauthenticated_returns_401() {
    let state = test_state().await;
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations/fake-id/share")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_share_non_owner_returns_404() {
    let state = test_state().await;
    let token1 = register_user(&state).await;
    let token2 = register_user2(&state).await;
    let conv_id = create_conv(&state, &token1).await;

    let resp = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token2,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn revoke_share_returns_204() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    // Create share first
    app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();

    let resp = app(state.clone())
        .oneshot(authed_delete(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn get_shared_conversation_returns_safe_fields() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    let resp = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    let share_body = json_body(resp).await;
    let share_token = share_body["share_token"].as_str().unwrap();

    let resp = app(state.clone())
        .oneshot(unauthed_get(&format!("/api/shared/{}", share_token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["title"].as_str().unwrap(), "Test Chat");
    // Must NOT contain sensitive fields
    assert!(body.get("user_id").is_none());
    assert!(body.get("system_prompt_override").is_none());
    assert!(body.get("provider").is_none());
}

#[tokio::test]
async fn get_shared_conversation_invalid_token_returns_404() {
    let state = test_state().await;
    let resp = app(state.clone())
        .oneshot(unauthed_get("/api/shared/nonexistent_token"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_shared_messages_returns_paginated() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    // Add a message directly
    db::messages::create_message(&state.db, &conv_id, "user", "Hello", None, None, None)
        .await
        .unwrap();

    // Share the conversation
    let resp = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    let share_body = json_body(resp).await;
    let share_token = share_body["share_token"].as_str().unwrap();

    let resp = app(state.clone())
        .oneshot(unauthed_get(&format!(
            "/api/shared/{}/messages?limit=10&offset=0",
            share_token
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["total"].as_i64().unwrap(), 1);
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["content"].as_str().unwrap(), "Hello");
}

#[tokio::test]
async fn revoked_share_returns_404() {
    let state = test_state().await;
    let token = register_user(&state).await;
    let conv_id = create_conv(&state, &token).await;

    // Create and revoke
    let resp = app(state.clone())
        .oneshot(authed_post(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();
    let share_body = json_body(resp).await;
    let share_token = share_body["share_token"].as_str().unwrap().to_string();

    app(state.clone())
        .oneshot(authed_delete(
            &format!("/api/conversations/{}/share", conv_id),
            &token,
        ))
        .await
        .unwrap();

    // Now accessing should 404
    let resp = app(state.clone())
        .oneshot(unauthed_get(&format!("/api/shared/{}", share_token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
