use axum::{
    Router,
    body::Body,
    extract::DefaultBodyLimit,
    http::{Request, StatusCode, header},
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
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

const MAX_BATCH_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;

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
        .nest(
            "/api/conversations/{id}/files",
            api::files::router().layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
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

fn authed_post_json(uri: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn authed_post_bytes(uri: &str, token: &str, content_type: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap()
}

async fn register_and_create_conversation(
    state: &Arc<AppState>,
    username: &str,
    email: &str,
) -> (String, String) {
    let response = app(state.clone())
        .oneshot(post_json(
            "/api/auth/register",
            &format!(r#"{{"username":"{username}","email":"{email}","password":"password123"}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();
    let user_id = body["user"]["id"].as_str().unwrap().to_string();

    let conversation = db::conversations::create_conversation(
        &state.db,
        &user_id,
        "Files tests",
        None,
        None,
        None,
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    (access_token, conversation.id)
}

fn multipart_body_single_file(filename: &str, content: &[u8]) -> (String, Vec<u8>) {
    let boundary = "XBOUNDARY1234567890";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[tokio::test]
async fn download_batch_rejects_too_many_paths() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "batchpathlimit", "batchpathlimit@example.com")
            .await;

    let paths: Vec<String> = (0..101).map(|i| format!("f{i}.txt")).collect();
    let request_body = serde_json::json!({ "paths": paths }).to_string();

    let response = app(state)
        .oneshot(authed_post_json(
            &format!("/api/conversations/{conv_id}/files/download-batch"),
            &token,
            &request_body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn download_batch_rejects_archive_over_size_limit() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "batchsizelimit", "batchsizelimit@example.com")
            .await;

    let conv_dir = format!("data/conversations/{conv_id}");
    tokio::fs::create_dir_all(&conv_dir).await.unwrap();
    let big_file = format!("{conv_dir}/big.bin");
    let file = tokio::fs::File::create(&big_file).await.unwrap();
    file.set_len(MAX_BATCH_DOWNLOAD_BYTES + 1).await.unwrap();

    let request_body = serde_json::json!({ "paths": ["big.bin"] }).to_string();
    let response = app(state)
        .oneshot(authed_post_json(
            &format!("/api/conversations/{conv_id}/files/download-batch"),
            &token,
            &request_body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn upload_streaming_saves_large_file_correctly() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "uploadstream", "uploadstream@example.com").await;

    let content: Vec<u8> = (0..(3 * 1024 * 1024 + 257))
        .map(|i| (i % 251) as u8)
        .collect();
    let (boundary, body) = multipart_body_single_file("large.bin", &content);

    let response = app(state.clone())
        .oneshot(authed_post_bytes(
            &format!("/api/conversations/{conv_id}/files/upload"),
            &token,
            &format!("multipart/form-data; boundary={boundary}"),
            body,
        ))
        .await
        .unwrap();
    let status = response.status();
    let payload = json_body(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected upload response: {payload}"
    );
    assert_eq!(payload["uploaded"][0]["name"], "large.bin");
    assert_eq!(payload["uploaded"][0]["size"], content.len() as u64);

    let saved = tokio::fs::read(format!("data/conversations/{conv_id}/large.bin"))
        .await
        .unwrap();
    assert_eq!(saved.len(), content.len());
    assert_eq!(sha256_hex(&saved), sha256_hex(&content));
}

#[tokio::test]
async fn view_file_with_valid_range_returns_partial_content() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "rangeok", "rangeok@example.com").await;

    let conv_dir = format!("data/conversations/{conv_id}");
    tokio::fs::create_dir_all(&conv_dir).await.unwrap();
    tokio::fs::write(format!("{conv_dir}/range.txt"), b"abcdefg")
        .await
        .unwrap();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{conv_id}/files/view?path=range.txt"
        ))
        .header("authorization", format!("Bearer {token}"))
        .header(header::RANGE, "bytes=1-3")
        .body(Body::empty())
        .unwrap();
    let response = app(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes 1-3/7"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"bcd");
}

#[tokio::test]
async fn view_file_with_invalid_range_falls_back_to_full_content() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "rangebad", "rangebad@example.com").await;

    let conv_dir = format!("data/conversations/{conv_id}");
    tokio::fs::create_dir_all(&conv_dir).await.unwrap();
    tokio::fs::write(format!("{conv_dir}/range.txt"), b"abcdefg")
        .await
        .unwrap();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{conv_id}/files/view?path=range.txt"
        ))
        .header("authorization", format!("Bearer {token}"))
        .header(header::RANGE, "bytes=100-200")
        .body(Body::empty())
        .unwrap();
    let response = app(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"abcdefg");
}

#[tokio::test]
async fn view_empty_file_with_range_returns_empty_full_response() {
    let state = test_state().await;
    let (token, conv_id) =
        register_and_create_conversation(&state, "rangeempty", "rangeempty@example.com").await;

    let conv_dir = format!("data/conversations/{conv_id}");
    tokio::fs::create_dir_all(&conv_dir).await.unwrap();
    tokio::fs::write(format!("{conv_dir}/empty.txt"), b"")
        .await
        .unwrap();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{conv_id}/files/view?path=empty.txt"
        ))
        .header("authorization", format!("Bearer {token}"))
        .header(header::RANGE, "bytes=0-0")
        .body(Body::empty())
        .unwrap();
    let response = app(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_LENGTH).unwrap(), "0");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.is_empty());
}
