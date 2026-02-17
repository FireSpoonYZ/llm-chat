# Backend Code Review

## Context

Comprehensive review of the Rust/Axum backend (~5000 lines, 30 files). Findings are prioritized by severity and focus on correctness, security, and maintainability.

---

## 1. Library Replacement Opportunities

### 1.1 [HIGH] No rate limiting on auth endpoints

**Location:** `src/api/auth.rs:16-22` (router definition), `src/main.rs:57` (nest without middleware)

`/register`, `/login`, `/refresh` are exposed with zero rate limiting. This is a brute-force and credential-stuffing vector. `tower` is already a dependency.

**Recommendation:** Add `tower-governor` (or `tower::limit::RateLimitLayer`) as middleware on the auth router. Rate-limit `/login` and `/register` to ~10 req/min per IP.

### 1.2 [MEDIUM] Hand-rolled input validation → `validator` crate

**Locations:**
- `src/api/auth.rs:55-60` — manual username length (3-50) and password length (≥8) checks
- `src/api/users.rs:100-107` — manual provider type whitelist
- `src/api/admin.rs:75-77` — manual transport type check

Scattered `if` statements with hardcoded rules. No email format validation at all on `RegisterRequest` (`src/api/auth.rs:24-29`).

**Recommendation:** Use the `validator` crate with `#[derive(Validate)]`. Centralizes rules, catches the missing email validation, and produces consistent error messages:
```rust
#[derive(Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 50))]
    pub username: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
}
```

### 1.3 [MEDIUM] Untyped outbound WebSocket messages → typed `Serialize` structs

**Locations:** `src/ws/client.rs` (15+ occurrences of `serde_json::json!{}`), `src/ws/container.rs` (6+ occurrences)

All outbound WS messages are built with `json!()` macros — field name typos are invisible at compile time. The inbound side already uses typed enums (`ClientMessage`, `ContainerMessage`).

**Recommendation:** Define `#[derive(Serialize)]` enums for outbound messages (e.g., `ServerMessage`, `ContainerCommand`) mirroring the inbound pattern. This gives compile-time guarantees on message shape.

---

## 2. Missing Test Coverage

### 2.1 [HIGH] Zero handler-level / integration tests for any API endpoint

No `backend/tests/` directory exists. None of the 7 API modules have handler tests:
- `src/api/auth.rs` — register, login, refresh, logout (security-critical)
- `src/api/users.rs` — profile, provider CRUD
- `src/api/conversations.rs` — conversation + message CRUD
- `src/api/files.rs` — list/download handlers (only helper functions tested)
- `src/api/admin.rs` — admin MCP server CRUD
- `src/api/presets.rs` — preset CRUD

The DB layer is well-tested (49+ tests across 6 modules), but the handler layer that wires auth + validation + DB + response serialization together has zero coverage.

**Recommendation:** Use `axum::test` helpers (or `tower::ServiceExt::oneshot`) to build integration tests. Priority order:
1. Auth endpoints (register validation, login success/failure, refresh rotation, expired token)
2. Conversation CRUD with ownership enforcement
3. Provider CRUD with API key masking and `__KEEP_EXISTING__` sentinel

### 2.2 [HIGH] Auth middleware extractors untested

- `src/auth/middleware.rs:28-55` — `AuthUser` extractor: 0 tests
- `src/auth/middleware.rs:63-76` — `AdminOnly` extractor: 0 tests

These are the security gatekeepers for every authenticated endpoint. Needed tests: valid token, expired token, missing `Authorization` header, malformed header (`Basic` instead of `Bearer`), non-admin accessing admin route.

### 2.3 [MEDIUM] WebSocket handlers untested

- `src/ws/client.rs` — 394 lines, 0 tests. Covers join, user_message, edit, regenerate, cancel.
- `src/ws/container.rs` — 295 lines, 0 tests. Covers ready, forward, complete, error.

The `WsState` connection management has 7 tests, but the actual message routing and business logic is untested.

### 2.4 [MEDIUM] Docker manager untested

- `src/docker/manager.rs` — 236 lines, 0 tests. `start_container`, `stop_container`, `cleanup_idle_containers`, `shutdown`.

Hard to unit test without mocking bollard, but the config assembly logic (env vars, volume mounts, network config) could be extracted into a pure function and tested.

### 2.5 [LOW] AppError → Response mapping untested

- `src/error.rs:32-49` — `IntoResponse` impl has 0 tests. Should verify each variant maps to the correct HTTP status code and that `Sqlx` errors don't leak DB details.

### 2.6 [LOW] WS message deserialization untested

- `src/ws/messages.rs` — `ClientMessage` and `ContainerMessage` enums with `#[serde(tag = "type")]` and `#[serde(other)]` have 0 tests. These attributes are easy to get wrong.

### 2.7 [LOW] WsState `pending_message` methods untested

- `src/ws/mod.rs:70-78` — `set_pending_message` / `take_pending_message` used in the critical container-startup path but have 0 tests.

---

## 3. Best Practice Violations

### 3.1 [HIGH] `AppError::Internal` leaks internal error details to clients

**Location:** `src/error.rs:41`
```rust
AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
```
The `#[error("Internal error: {0}")]` format sends raw error strings (file paths, crypto errors, reqwest errors, Docker API details) to the client. Compare with the `Sqlx` variant (line 42-44) which correctly logs and returns a generic message.

Affected call sites: `api/auth.rs:70,75,107,114,147,161`, `api/users.rs:121`, `api/files.rs:69,74,79,204,245,302,304,306,309,313`.

**Fix:** Log the real error via `tracing::error!` and return a generic `"Internal server error"` string, matching the `Sqlx` pattern.

### 3.2 [HIGH] Blocking `std::fs` calls in async context

Synchronous filesystem operations on the tokio runtime block the executor thread:
- `src/docker/manager.rs:61` — `std::fs::create_dir_all`
- `src/docker/manager.rs:66` — `std::fs::canonicalize`
- `src/api/conversations.rs:84` — `std::fs::create_dir_all`
- `src/api/conversations.rs:156` — `std::fs::remove_dir_all` (can be slow for large dirs)
- `src/api/files.rs:332-339` — `std::fs::read_dir` + `std::fs::read` in `add_dir_to_zip` (reads potentially many files synchronously)

**Fix:** Use `tokio::fs` equivalents, or wrap in `tokio::task::spawn_blocking`. The `add_dir_to_zip` function is the worst offender — it should be moved to a blocking task entirely.

### 3.3 [HIGH] Argon2 password hashing blocks the async executor

**Location:** `src/api/auth.rs:69-70` (`hash_password`), `src/api/auth.rs:106-107` (`verify_password`)

Argon2 is deliberately CPU-intensive. Running it directly in an async handler blocks the tokio thread pool.

**Fix:** Wrap in `tokio::task::spawn_blocking`:
```rust
let hash = tokio::task::spawn_blocking(move || password::hash_password(&pw))
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .map_err(AppError::Internal)?;
```

### 3.4 [MEDIUM] Container error message leaks Docker internals to client

**Location:** `src/ws/client.rs:63`
```rust
"message": format!("Failed to start container: {e}")
```
The bollard error `e` can contain Docker API details (image names, network config, host paths).

**Fix:** Log the full error server-side, send a generic message to the client.

### 3.5 [MEDIUM] `CorsLayer::permissive()` allows all origins

**Location:** `src/main.rs:48-51`
```rust
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);
```

**Fix:** Make allowed origins configurable via `Config` (e.g., `CORS_ALLOWED_ORIGINS` env var). Default to restrictive in production.

### 3.6 [MEDIUM] `Result<T, String>` instead of typed errors

**Locations:**
- `src/crypto.rs:12,45` — `encrypt`/`decrypt` return `Result<String, String>`
- `src/auth/password.rs:8,21` — `hash_password`/`verify_password` return `Result<_, String>`
- `src/docker/manager.rs:36,154` — `start_container`/`stop_container` return `Result<_, String>`

This loses type information and forces callers to use `.map_err(AppError::Internal)` everywhere. `thiserror` is already a dependency.

**Fix:** Define proper error enums with `#[derive(thiserror::Error)]` and implement `From` conversions to `AppError`.

### 3.7 [MEDIUM] `ContainerClaims.single_use` is declared but never enforced

**Location:** `src/auth/mod.rs:35` (field definition), `src/auth/mod.rs:72` (always set to `false`), `src/ws/container.rs:28-31` (never checked)

Dead code suggesting an incomplete security feature.

**Fix:** Either implement single-use enforcement or remove the field.

### 3.8 [MEDIUM] `.unwrap()` on `Response::builder().body()` in production code

**Locations:**
- `src/api/files.rs:220` — `.body(body).unwrap()`
- `src/api/files.rs:266` — `.body(body).unwrap()`
- `src/api/files.rs:323` — `.body(Body::from(bytes)).unwrap()`

While unlikely to fail with hardcoded headers, `.unwrap()` in request handlers is a panic risk.

**Fix:** Use `.map_err(|e| AppError::Internal(e.to_string()))?` instead.

### 3.9 [MEDIUM] Hardcoded JWT expiration times

**Locations:**
- `src/auth/mod.rs:50` — access token: 2 hours
- `src/auth/mod.rs:70` — container token: 1 hour
- `src/api/auth.rs:78,117,164` — refresh token: 30 days

**Fix:** Move to `Config` fields so they can be tuned per environment.

### 3.10 [MEDIUM] No email validation on registration

**Location:** `src/api/auth.rs:24-29` — `RegisterRequest` has `email: String` with no format validation. Users can register with `email: ""` or `email: "not-an-email"`.

**Fix:** Add `#[validate(email)]` (ties into finding 1.2).

### 3.11 [LOW] Inconsistent pagination limits in WS vs REST

- `src/api/conversations.rs:196` — REST caps at 100: `params.limit.unwrap_or(50).min(100)`
- `src/ws/client.rs:256,322` — hardcoded `1000` with no cap
- `src/ws/container.rs:113-114` — hardcoded `50`

**Fix:** Use a shared constant or config value.

---

## Summary

| Severity | Count | Key Items |
|----------|-------|-----------|
| HIGH | 6 | Rate limiting, internal error leakage, blocking I/O in async, Argon2 blocking, no handler tests, no auth middleware tests |
| MEDIUM | 10 | Validator crate, typed WS messages, container error leak, CORS, `Result<String>`, dead `single_use`, `.unwrap()`, JWT hardcoding, email validation, Docker manager tests |
| LOW | 4 | AppError tests, WS message deser tests, pending_message tests, pagination inconsistency |
