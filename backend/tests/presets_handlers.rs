use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use claude_chat_backend::{
    api, auth,
    auth::middleware::AppState,
    config::Config,
    db,
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
        .nest("/api/presets", api::presets::router())
        .with_state(state)
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

fn put_json_with_auth(uri: &str, body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body.to_string()))
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

fn get_with_auth(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn register_user(state: &Arc<AppState>, username: &str, email: &str) -> (String, String) {
    let resp = app(state.clone())
        .oneshot(post_json(
            "/api/auth/register",
            &format!(
                r#"{{"username":"{}","email":"{}","password":"password123"}}"#,
                username, email
            ),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    (
        body["access_token"].as_str().unwrap().to_string(),
        body["user"]["id"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn register_initializes_all_builtin_presets() {
    let state = test_state().await;
    let (token, _uid) = register_user(&state, "preset_init", "preset_init@example.com").await;

    let resp = app(state.clone())
        .oneshot(get_with_auth("/api/presets", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    let presets = body.as_array().expect("presets should be an array");
    assert_eq!(presets.len(), 4);

    let names: Vec<&str> = presets
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"Default"));
    assert!(names.contains(&"Claude AI"));
    assert!(names.contains(&"Claude Code"));
    assert!(names.contains(&"Claude Cowork"));

    let defaults: Vec<&serde_json::Value> = presets
        .iter()
        .filter(|p| p.get("is_default").and_then(|v| v.as_bool()) == Some(true))
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(
        defaults[0].get("name").and_then(|v| v.as_str()),
        Some("Default")
    );

    let builtin_ids: Vec<&str> = presets
        .iter()
        .filter_map(|p| p.get("builtin_id").and_then(|v| v.as_str()))
        .collect();
    assert!(builtin_ids.contains(&"default"));
    assert!(builtin_ids.contains(&"claude-ai"));
    assert!(builtin_ids.contains(&"claude-code"));
    assert!(builtin_ids.contains(&"claude-cowork"));
}

#[tokio::test]
async fn list_presets_is_read_only_for_existing_user() {
    let state = test_state().await;
    let (token, uid) =
        register_user(&state, "preset_readonly", "preset_readonly@example.com").await;

    // Delete all presets to emulate a non-standard user state.
    sqlx::query("DELETE FROM user_presets WHERE user_id = ?")
        .bind(&uid)
        .execute(&state.db)
        .await
        .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth("/api/presets", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn duplicate_name_presets_are_managed_by_id() {
    let state = test_state().await;
    let (token, _uid) = register_user(&state, "preset_dupe", "preset_dupe@example.com").await;

    let create_payload = r#"{"name":"Same Name","description":"v1","content":"content-v1"}"#;
    let resp1 = app(state.clone())
        .oneshot(post_json_with_auth("/api/presets", create_payload, &token))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let body1 = json_body(resp1).await;
    let id1 = body1["id"].as_str().unwrap().to_string();

    let create_payload = r#"{"name":"Same Name","description":"v2","content":"content-v2"}"#;
    let resp2 = app(state.clone())
        .oneshot(post_json_with_auth("/api/presets", create_payload, &token))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::CREATED);
    let body2 = json_body(resp2).await;
    let id2 = body2["id"].as_str().unwrap().to_string();
    assert_ne!(id1, id2);

    let update_resp = app(state.clone())
        .oneshot(put_json_with_auth(
            &format!("/api/presets/{}", id1),
            r#"{"description":"updated-v1"}"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);

    let del_resp = app(state.clone())
        .oneshot(delete_with_auth(&format!("/api/presets/{}", id1), &token))
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    let list_resp = app(state.clone())
        .oneshot(get_with_auth("/api/presets", &token))
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = json_body(list_resp).await;
    let presets = list_body.as_array().unwrap();
    assert!(presets.iter().any(|p| p["id"] == id2));
    assert!(!presets.iter().any(|p| p["id"] == id1));
}

fn post_json_with_auth(uri: &str, body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn presets_endpoint_does_not_seed_for_non_register_user_creation_path() {
    let state = test_state().await;
    let user = db::users::create_user(&state.db, "manual_user", "manual_user@example.com", "hash")
        .await
        .unwrap();

    let token = auth::create_access_token(
        &user.id,
        &user.username,
        user.is_admin,
        &state.config.jwt_secret,
        state.config.access_token_ttl_secs,
    )
    .unwrap();

    let resp = app(state.clone())
        .oneshot(get_with_auth("/api/presets", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}
