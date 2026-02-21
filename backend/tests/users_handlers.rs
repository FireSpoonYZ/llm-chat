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
        .nest("/api/users", api::users::router())
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

fn post_with_auth(uri: &str, body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn put_with_auth(uri: &str, body: &str, token: &str) -> Request<Body> {
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

async fn create_provider(state: &Arc<AppState>, token: &str, body: &str) -> serde_json::Value {
    let resp = app(state.clone())
        .oneshot(post_with_auth("/api/users/me/providers", body, token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    json_body(resp).await
}

#[tokio::test]
async fn upsert_provider_accepts_image_only_models() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let body = create_provider(
        &state,
        &token,
        r#"{
            "name":"My Google",
            "provider_type":"google",
            "api_key":"k1",
            "models":[],
            "image_models":["gemini-image-v1"],
            "is_default": false
        }"#,
    )
    .await;

    assert!(body["models"].as_array().is_some_and(|arr| arr.is_empty()));
    assert_eq!(body["image_models"][0], "gemini-image-v1");
}

#[tokio::test]
async fn upsert_provider_rejects_when_both_model_lists_are_empty() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let resp = app(state.clone())
        .oneshot(post_with_auth(
            "/api/users/me/providers",
            r#"{
                "name":"Empty Provider",
                "provider_type":"openai",
                "api_key":"k1",
                "models":[],
                "image_models":[],
                "is_default": false
            }"#,
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_model_defaults_initializes_from_legacy_default_provider() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let created = create_provider(
        &state,
        &token,
        r#"{
            "name":"Legacy Default",
            "provider_type":"openai",
            "api_key":"k-openai",
            "models":["gpt-4o"],
            "image_models":[],
            "is_default": true
        }"#,
    )
    .await;

    let get_resp = app(state.clone())
        .oneshot(get_with_auth("/api/users/me/model-defaults", &token))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = json_body(get_resp).await;
    assert_eq!(body["chat_provider_id"], created["id"]);
    assert_eq!(body["chat_model"], "gpt-4o");
    assert_eq!(body["subagent_provider_id"], created["id"]);
    assert_eq!(body["subagent_model"], "gpt-4o");
    assert!(body["image_provider_id"].is_null());
    assert!(body["image_model"].is_null());
}

#[tokio::test]
async fn upsert_provider_keeps_existing_default_when_is_default_not_provided() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let created = create_provider(
        &state,
        &token,
        r#"{
            "name":"Sticky Default",
            "provider_type":"openai",
            "api_key":"k-openai",
            "models":["gpt-4o"],
            "image_models":[],
            "is_default": true
        }"#,
    )
    .await;

    let update_resp = app(state.clone())
        .oneshot(post_with_auth(
            "/api/users/me/providers",
            &format!(
                r#"{{
                    "id":"{}",
                    "name":"Sticky Default Renamed",
                    "provider_type":"openai",
                    "api_key":"__KEEP_EXISTING__",
                    "models":["gpt-5"],
                    "image_models":[]
                }}"#,
                created["id"].as_str().unwrap()
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);
    let body = json_body(update_resp).await;
    assert_eq!(body["is_default"], true);
}

#[tokio::test]
async fn get_and_update_model_defaults_roundtrip() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let main = create_provider(
        &state,
        &token,
        r#"{
            "name":"Main OpenAI",
            "provider_type":"openai",
            "api_key":"k-openai",
            "models":["gpt-4o"],
            "image_models":[],
            "is_default": false
        }"#,
    )
    .await;
    let sub = create_provider(
        &state,
        &token,
        r#"{
            "name":"Sub Anthropic",
            "provider_type":"anthropic",
            "api_key":"k-anthropic",
            "models":["claude-3-opus"],
            "image_models":[],
            "is_default": false
        }"#,
    )
    .await;
    let img = create_provider(
        &state,
        &token,
        r#"{
            "name":"Img Google",
            "provider_type":"google",
            "api_key":"k-google",
            "models":["gemini-2.5-pro"],
            "image_models":["gemini-image-v1"],
            "is_default": false
        }"#,
    )
    .await;

    let update_resp = app(state.clone())
        .oneshot(put_with_auth(
            "/api/users/me/model-defaults",
            &format!(
                r#"{{
                    "chat_provider_id":"{}",
                    "chat_model":"gpt-4o",
                    "subagent_provider_id":"{}",
                    "subagent_model":"claude-3-opus",
                    "image_provider_id":"{}",
                    "image_model":"gemini-image-v1"
                }}"#,
                main["id"].as_str().unwrap(),
                sub["id"].as_str().unwrap(),
                img["id"].as_str().unwrap(),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);

    let get_resp = app(state.clone())
        .oneshot(get_with_auth("/api/users/me/model-defaults", &token))
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = json_body(get_resp).await;
    assert_eq!(body["chat_provider_id"], main["id"]);
    assert_eq!(body["subagent_provider_id"], sub["id"]);
    assert_eq!(body["image_model"], "gemini-image-v1");
}

#[tokio::test]
async fn update_model_defaults_requires_main_and_subagent() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let main = create_provider(
        &state,
        &token,
        r#"{
            "name":"Main OpenAI",
            "provider_type":"openai",
            "api_key":"k-openai",
            "models":["gpt-4o"],
            "image_models":[],
            "is_default": false
        }"#,
    )
    .await;

    let resp = app(state.clone())
        .oneshot(put_with_auth(
            "/api/users/me/model-defaults",
            &format!(
                r#"{{
                    "chat_provider_id":"{}",
                    "chat_model":"gpt-4o",
                    "subagent_provider_id":"",
                    "subagent_model":""
                }}"#,
                main["id"].as_str().unwrap(),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn deleting_provider_clears_referenced_defaults() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let shared = create_provider(
        &state,
        &token,
        r#"{
            "name":"Shared Provider",
            "provider_type":"openai",
            "api_key":"k1",
            "models":["gpt-4o"],
            "image_models":["img-v1"],
            "is_default": false
        }"#,
    )
    .await;

    let update_defaults = app(state.clone())
        .oneshot(put_with_auth(
            "/api/users/me/model-defaults",
            &format!(
                r#"{{
                    "chat_provider_id":"{}",
                    "chat_model":"gpt-4o",
                    "subagent_provider_id":"{}",
                    "subagent_model":"gpt-4o",
                    "image_provider_id":"{}",
                    "image_model":"img-v1"
                }}"#,
                shared["id"].as_str().unwrap(),
                shared["id"].as_str().unwrap(),
                shared["id"].as_str().unwrap(),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_defaults.status(), StatusCode::OK);

    let delete_resp = app(state.clone())
        .oneshot(delete_with_auth(
            &format!("/api/users/me/providers/{}", shared["id"].as_str().unwrap()),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    let get_resp = app(state.clone())
        .oneshot(get_with_auth("/api/users/me/model-defaults", &token))
        .await
        .unwrap();
    let body = json_body(get_resp).await;
    assert!(body["chat_provider_id"].is_null());
    assert!(body["chat_model"].is_null());
    assert!(body["subagent_provider_id"].is_null());
    assert!(body["subagent_model"].is_null());
    assert!(body["image_provider_id"].is_null());
    assert!(body["image_model"].is_null());
}

#[tokio::test]
async fn provider_update_prunes_invalid_model_defaults() {
    let state = test_state().await;
    let token = register_user(&state).await;

    let mutable = create_provider(
        &state,
        &token,
        r#"{
            "name":"Mutable Provider",
            "provider_type":"openai",
            "api_key":"k1",
            "models":["gpt-4o"],
            "image_models":["img-v1"],
            "is_default": false
        }"#,
    )
    .await;

    let update_defaults = app(state.clone())
        .oneshot(put_with_auth(
            "/api/users/me/model-defaults",
            &format!(
                r#"{{
                    "chat_provider_id":"{}",
                    "chat_model":"gpt-4o",
                    "subagent_provider_id":"{}",
                    "subagent_model":"gpt-4o",
                    "image_provider_id":"{}",
                    "image_model":"img-v1"
                }}"#,
                mutable["id"].as_str().unwrap(),
                mutable["id"].as_str().unwrap(),
                mutable["id"].as_str().unwrap(),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_defaults.status(), StatusCode::OK);

    let update_provider = app(state.clone())
        .oneshot(post_with_auth(
            "/api/users/me/providers",
            &format!(
                r#"{{
                    "id":"{}",
                    "name":"Mutable Provider",
                    "provider_type":"openai",
                    "api_key":"__KEEP_EXISTING__",
                    "models":["gpt-5"],
                    "image_models":["img-v2"],
                    "is_default": false
                }}"#,
                mutable["id"].as_str().unwrap(),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(update_provider.status(), StatusCode::OK);

    let get_resp = app(state.clone())
        .oneshot(get_with_auth("/api/users/me/model-defaults", &token))
        .await
        .unwrap();
    let body = json_body(get_resp).await;
    assert!(body["chat_provider_id"].is_null());
    assert!(body["chat_model"].is_null());
    assert!(body["subagent_provider_id"].is_null());
    assert!(body["subagent_model"].is_null());
    assert!(body["image_provider_id"].is_null());
    assert!(body["image_model"].is_null());
}
