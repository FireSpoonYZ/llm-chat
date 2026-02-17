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
        cookie_secure: false,
    }
}

/// Build a test AppState with an in-memory SQLite DB.
async fn test_state() -> Arc<AppState> {
    test_state_with_config(test_config()).await
}

async fn test_state_with_config(config: Config) -> Arc<AppState> {
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

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn extract_cookie(response: &axum::response::Response, name: &str) -> Option<String> {
    for value in response.headers().get_all("set-cookie").iter() {
        let cookie = value.to_str().ok()?;
        if let Some(rest) = cookie.strip_prefix(&format!("{name}=")) {
            let raw = rest.split(';').next().unwrap_or_default();
            return Some(format!("{name}={raw}"));
        }
    }
    None
}

fn set_cookie_values(response: &axum::response::Response) -> Vec<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok().map(ToOwned::to_owned))
        .collect()
}

fn cookie_value_for<'a>(cookies: &'a [String], name: &str) -> Option<&'a str> {
    cookies
        .iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(String::as_str)
}

// ── Register ──

#[tokio::test]
async fn register_success() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"alice","email":"alice@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["email"], "alice@example.com");
}

#[tokio::test]
async fn register_sets_http_only_lax_auth_cookies() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"cookiecheck","email":"cookiecheck@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookies = set_cookie_values(&resp);
    let access = cookie_value_for(&cookies, "access_token").expect("missing access cookie");
    let refresh = cookie_value_for(&cookies, "refresh_token").expect("missing refresh cookie");

    for cookie in [access, refresh] {
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(!cookie.contains("Secure"));
        assert!(cookie.contains("Max-Age="));
    }
    assert!(access.contains("Max-Age=7200"));
    assert!(refresh.contains("Max-Age=2592000"));
}

#[tokio::test]
async fn register_sets_secure_cookie_when_config_enabled() {
    let mut config = test_config();
    config.cookie_secure = true;
    let state = test_state_with_config(config).await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"securecookie","email":"securecookie@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookies = set_cookie_values(&resp);
    let access = cookie_value_for(&cookies, "access_token").expect("missing access cookie");
    let refresh = cookie_value_for(&cookies, "refresh_token").expect("missing refresh cookie");
    assert!(access.contains("Secure"));
    assert!(refresh.contains("Secure"));
}

#[tokio::test]
async fn register_short_username_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"ab","email":"ab@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_short_password_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"alice","email":"alice@example.com","password":"short"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_invalid_email_rejected() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"alice","email":"not-an-email","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_duplicate_username_rejected() {
    let state = test_state().await;

    // Register first user
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"alice","email":"alice@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Try duplicate username
    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"alice","email":"alice2@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn concurrent_register_same_username_one_conflict() {
    let state = test_state().await;

    let app1 = auth_app(state.clone());
    let app2 = auth_app(state.clone());
    let fut1 = app1.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"raceuser","email":"raceuser1@example.com","password":"password123"}"#,
    ));
    let fut2 = app2.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"raceuser","email":"raceuser2@example.com","password":"password123"}"#,
    ));
    let (resp1, resp2) = tokio::join!(fut1, fut2);

    let status1 = resp1.unwrap().status();
    let status2 = resp2.unwrap().status();
    assert!(
        (status1 == StatusCode::OK && status2 == StatusCode::CONFLICT)
            || (status2 == StatusCode::OK && status1 == StatusCode::CONFLICT),
        "expected one success and one conflict, got ({status1}, {status2})"
    );
}

#[tokio::test]
async fn concurrent_register_same_email_one_conflict() {
    let state = test_state().await;

    let app1 = auth_app(state.clone());
    let app2 = auth_app(state.clone());
    let fut1 = app1.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"raceemail1","email":"raceemail@example.com","password":"password123"}"#,
    ));
    let fut2 = app2.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"raceemail2","email":"raceemail@example.com","password":"password123"}"#,
    ));
    let (resp1, resp2) = tokio::join!(fut1, fut2);

    let status1 = resp1.unwrap().status();
    let status2 = resp2.unwrap().status();
    assert!(
        (status1 == StatusCode::OK && status2 == StatusCode::CONFLICT)
            || (status2 == StatusCode::OK && status1 == StatusCode::CONFLICT),
        "expected one success and one conflict, got ({status1}, {status2})"
    );
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
    ))
    .await
    .unwrap();

    // Login
    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"bob","password":"password123"}"#,
        ))
        .await
        .unwrap();
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
    ))
    .await
    .unwrap();

    // Login with wrong password
    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"carol","password":"wrongpassword"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_nonexistent_user() {
    let state = test_state().await;
    let app = auth_app(state);

    let resp = app
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"nobody","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_sets_auth_cookies_with_expected_attributes() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    app.oneshot(post_json(
        "/api/auth/register",
        r#"{"username":"logincookie","email":"logincookie@example.com","password":"password123"}"#,
    ))
    .await
    .unwrap();

    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"logincookie","password":"password123"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookies = set_cookie_values(&resp);
    let access = cookie_value_for(&cookies, "access_token").expect("missing access cookie");
    let refresh = cookie_value_for(&cookies, "refresh_token").expect("missing refresh cookie");
    for cookie in [access, refresh] {
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Max-Age="));
    }
    assert!(access.contains("Max-Age=7200"));
    assert!(refresh.contains("Max-Age=2592000"));
}

// ── Refresh ──

#[tokio::test]
async fn refresh_token_rotation() {
    let state = test_state().await;

    // Register and get tokens
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"dave","email":"dave@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Use refresh token
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    let new_refresh_token = body["refresh_token"].as_str().unwrap().to_string();
    // New token should be different (rotation)
    assert_ne!(new_refresh_token, refresh_token);

    // Old token should no longer work
    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_accepts_refresh_cookie_without_body_token() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"cookie-refresh","email":"cookie-refresh@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let refresh_cookie = extract_cookie(&resp, "refresh_token").expect("missing refresh cookie");

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .header("cookie", refresh_cookie)
        .body(Body::from("{}"))
        .unwrap();

    let app = auth_app(state);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn refresh_sets_auth_cookies_with_expected_attributes() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"refreshcookie","email":"refreshcookie@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap();

    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookies = set_cookie_values(&resp);
    let access = cookie_value_for(&cookies, "access_token").expect("missing access cookie");
    let refresh = cookie_value_for(&cookies, "refresh_token").expect("missing refresh cookie");
    for cookie in [access, refresh] {
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Max-Age="));
    }
    assert!(access.contains("Max-Age=7200"));
    assert!(refresh.contains("Max-Age=2592000"));
}

#[tokio::test]
async fn refresh_rotation_rolls_back_when_new_token_insert_fails() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"rollback","email":"rollback@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    sqlx::query(
        "CREATE TRIGGER fail_refresh_insert
         BEFORE INSERT ON refresh_tokens
         BEGIN
             SELECT RAISE(FAIL, 'forced refresh insert failure');
         END;",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let app = auth_app(state.clone());
    let failed_resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(failed_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    sqlx::query("DROP TRIGGER fail_refresh_insert")
        .execute(&state.db)
        .await
        .unwrap();

    // If transaction rollback worked, the original refresh token was not consumed.
    let app = auth_app(state);
    let success_resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(success_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn refresh_token_concurrent_replay_only_one_succeeds() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"dave2","email":"dave2@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    let payload = format!(r#"{{"refresh_token":"{}"}}"#, refresh_token);
    let app1 = auth_app(state.clone());
    let app2 = auth_app(state.clone());
    let fut1 = app1.oneshot(post_json("/api/auth/refresh", &payload));
    let fut2 = app2.oneshot(post_json("/api/auth/refresh", &payload));
    let (resp1, resp2) = tokio::join!(fut1, fut2);

    let status1 = resp1.unwrap().status();
    let status2 = resp2.unwrap().status();
    assert!(
        (status1 == StatusCode::OK && status2 == StatusCode::UNAUTHORIZED)
            || (status2 == StatusCode::OK && status1 == StatusCode::UNAUTHORIZED),
        "expected one success and one unauthorized, got ({status1}, {status2})"
    );
}

// ── Logout ──

#[tokio::test]
async fn logout_invalidates_refresh_token() {
    let state = test_state().await;

    // Register
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"eve","email":"eve@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Logout
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/logout",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Refresh should fail after logout
    let app = auth_app(state);
    let resp = app
        .oneshot(post_json(
            "/api/auth/refresh",
            &format!(r#"{{"refresh_token":"{}"}}"#, refresh_token),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_with_refresh_cookie_clears_auth_cookies() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"cookie-logout","email":"cookie-logout@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let refresh_cookie = extract_cookie(&resp, "refresh_token").expect("missing refresh cookie");

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .header("cookie", refresh_cookie.clone())
        .body(Body::from("{}"))
        .unwrap();

    let app = auth_app(state.clone());
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookies = set_cookie_values(&resp);
    let cleared_access =
        cookie_value_for(&cookies, "access_token").expect("missing cleared access cookie");
    let cleared_refresh =
        cookie_value_for(&cookies, "refresh_token").expect("missing cleared refresh cookie");
    assert!(cleared_access.contains("Max-Age=0"));
    assert!(cleared_refresh.contains("Max-Age=0"));
    assert!(cleared_access.contains("Path=/"));
    assert!(cleared_refresh.contains("Path=/"));
    assert!(cleared_access.contains("HttpOnly"));
    assert!(cleared_refresh.contains("HttpOnly"));
    assert!(cleared_access.contains("SameSite=Lax"));
    assert!(cleared_refresh.contains("SameSite=Lax"));

    // Original refresh cookie should no longer work after logout.
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .header("cookie", refresh_cookie)
        .body(Body::from("{}"))
        .unwrap();
    let app = auth_app(state);
    let resp = app.oneshot(req).await.unwrap();
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
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"frank","email":"frank@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();

    // Use token to access protected endpoint
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);
    let resp = app
        .oneshot(get_with_auth("/api/conversations", &access_token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn authenticated_endpoint_with_invalid_token_returns_401() {
    let state = test_state().await;
    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let resp = app
        .oneshot(get_with_auth("/api/conversations", "invalid-token-here"))
        .await
        .unwrap();
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

// ── Cookie Auth ──

#[tokio::test]
async fn cookie_authenticates_successfully() {
    let state = test_state().await;

    // Register to obtain auth cookies.
    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"grace","email":"grace@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let access_cookie = extract_cookie(&resp, "access_token").expect("missing access cookie");

    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);
    let resp = app
        .oneshot(get_with_cookie("/api/conversations", &access_cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn query_param_token_is_rejected() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"queryonly","email":"queryonly@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();

    let app = Router::new()
        .nest("/api/conversations", api::conversations::router())
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/conversations?token={access_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cookie_token_supported_for_file_view() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"heidi","email":"heidi@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let access_cookie = extract_cookie(&resp, "access_token").expect("missing access cookie");
    let body = json_body(resp).await;
    let user_id = body["user"]["id"].as_str().unwrap().to_string();

    let conversation = db::conversations::create_conversation(
        &state.db, &user_id, "Files", None, None, None, false, None, None, None,
    )
    .await
    .unwrap();

    let dir = format!("data/conversations/{}", conversation.id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(format!("{dir}/hello.txt"), b"hello")
        .await
        .unwrap();

    let app = Router::new()
        .nest("/api/conversations/{id}/files", api::files::router())
        .with_state(state);
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{}/files/view?path=hello.txt",
            conversation.id
        ))
        .header("cookie", access_cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn query_token_supported_for_file_view() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"queryfileok","email":"queryfileok@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let access_token = body["access_token"].as_str().unwrap().to_string();
    let user_id = body["user"]["id"].as_str().unwrap().to_string();

    let conversation = db::conversations::create_conversation(
        &state.db, &user_id, "Files", None, None, None, false, None, None, None,
    )
    .await
    .unwrap();

    let dir = format!("data/conversations/{}", conversation.id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(format!("{dir}/hello.txt"), b"hello")
        .await
        .unwrap();

    let app = Router::new()
        .nest("/api/conversations/{id}/files", api::files::router())
        .with_state(state);
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{}/files/view?path=hello.txt&token={}",
            conversation.id, access_token
        ))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_query_token_for_file_view_returns_401() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"queryfilebad","email":"queryfilebad@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let user_id = body["user"]["id"].as_str().unwrap().to_string();

    let conversation = db::conversations::create_conversation(
        &state.db, &user_id, "Files", None, None, None, false, None, None, None,
    )
    .await
    .unwrap();

    let dir = format!("data/conversations/{}", conversation.id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(format!("{dir}/hello.txt"), b"hello")
        .await
        .unwrap();

    let app = Router::new()
        .nest("/api/conversations/{id}/files", api::files::router())
        .with_state(state);
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{}/files/view?path=hello.txt&token=invalid-token",
            conversation.id
        ))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_auth_for_file_view_returns_401() {
    let state = test_state().await;

    let app = auth_app(state.clone());
    let resp = app
        .oneshot(post_json(
            "/api/auth/register",
            r#"{"username":"queryfilenone","email":"queryfilenone@example.com","password":"password123"}"#,
        ))
        .await
        .unwrap();
    let body = json_body(resp).await;
    let user_id = body["user"]["id"].as_str().unwrap().to_string();

    let conversation = db::conversations::create_conversation(
        &state.db, &user_id, "Files", None, None, None, false, None, None, None,
    )
    .await
    .unwrap();

    let dir = format!("data/conversations/{}", conversation.id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(format!("{dir}/hello.txt"), b"hello")
        .await
        .unwrap();

    let app = Router::new()
        .nest("/api/conversations/{id}/files", api::files::router())
        .with_state(state);
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/conversations/{}/files/view?path=hello.txt",
            conversation.id
        ))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
