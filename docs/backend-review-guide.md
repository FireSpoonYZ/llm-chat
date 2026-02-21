# Rust/Axum 后端 Code Review 指导文档

本文档为 Rust/Axum 后端项目的代码审查提供系统化的检查指南。聚焦技术栈 best practice，不涉及具体业务逻辑。

---

## 1. 技术栈概览

| 类别 | 技术 | 用途 |
|------|------|------|
| 语言/运行时 | Rust 2024 + Tokio | 系统编程语言 + 异步运行时 |
| Web 框架 | Axum 0.8 | HTTP 路由、WebSocket、Extractor 模式 |
| 数据库 | SQLx 0.8 + SQLite | 异步数据库访问，编译期 SQL 检查 |
| 容器管理 | Bollard | Docker Engine API 客户端 |
| 认证 | jsonwebtoken + Argon2 | JWT 令牌 + 密码哈希 |
| 加密 | AES-256-GCM | 敏感数据加密存储 |
| 中间件 | tower-http, tower_governor | CORS、Tracing、限流 |
| 错误处理 | thiserror | 结构化错误类型派生 |
| 序列化 | serde / serde_json | JSON 序列化/反序列化 |
| 校验 | validator | 请求体字段校验 |
| 并发 | DashMap, tokio::sync | 并发安全数据结构 |
| 可观测性 | tracing + tracing-subscriber | 结构化日志 |
| HTTP 客户端 | reqwest (rustls) | 外部 HTTP 请求 |
| 工具 | uuid, chrono, futures-util | ID 生成、时间处理、Stream 组合 |

---

## 2. 代码质量

### 2.1 模块结构与职责分离

**检查要点：** 代码是否遵循分层架构，各层职责是否清晰。

推荐的分层结构：
- `api/` — HTTP handler，只做请求解析、校验、调用下层、构造响应
- `db/` — 数据访问层，纯函数式，接收连接池参数
- `ws/` — WebSocket 连接管理与消息路由
- `docker/` — 容器生命周期管理
- `auth/` — 认证/授权逻辑与中间件

```rust
// ✅ db 层函数：纯粹的数据访问，接收 pool 参数
pub async fn get_item(pool: &SqlitePool, id: &str) -> Result<Item, sqlx::Error> {
    sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
}

// ❌ db 层不应包含 HTTP 逻辑
pub async fn get_item(pool: &SqlitePool, id: &str) -> Result<Json<Item>, StatusCode> {
    // 不要在 db 层返回 HTTP 类型
}
```

### 2.2 类型系统与数据建模

- [ ] 请求/响应结构体是否与 DB 模型分离（`CreateItemRequest` vs `Item` vs `ItemResponse`）
- [ ] DB 行结构体是否使用 `#[derive(FromRow)]`，字段类型与 SQLite 列类型匹配
- [ ] 可空字段是否正确使用 `Option<T>`，避免空字符串与 `None` 混淆
- [ ] 类型转换是否使用 `From` / `Into` trait

```rust
// ✅ 请求/响应/DB 模型分离
#[derive(Deserialize, Validate)]
pub struct CreateItemRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
}

#[derive(FromRow)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ItemResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    // 注意：不暴露 user_id 等内部字段
}

impl From<Item> for ItemResponse {
    fn from(item: Item) -> Self {
        Self { id: item.id, name: item.name, created_at: item.created_at }
    }
}
```

### 2.3 命名惯例

- [ ] 类型使用 `PascalCase`，函数/变量使用 `snake_case`，常量使用 `UPPER_SNAKE_CASE`
- [ ] 函数命名遵循 `verb_noun` 模式（`create_user`, `list_conversations`）
- [ ] getter 不使用 `get_` 前缀（Rust 惯例：`fn name(&self)` 而非 `fn get_name(&self)`）
- [ ] 转换方法遵循 `as_`（借用）/ `to_`（克隆/转换）/ `into_`（消费）前缀约定
- [ ] 路由路径使用 kebab-case（`/mcp-servers`），Axum 0.8 路径参数使用 `{id}` 语法

### 2.4 Clippy 与编译警告

- [ ] `unwrap()` 仅出现在初始化阶段，运行时路径全部使用 `?` 或 `expect("reason")`
- [ ] 无未使用的 `use` 导入和 `dead_code`
- [ ] `#[allow(...)]` 标注有正当理由
- [ ] 参数超过 7 个时考虑引入 builder 或配置结构体

```rust
// ✅ 运行时使用 ? 传播错误
let config = Config::from_env().expect("Failed to load config"); // 启动阶段可以 expect
let user = db::get_user(pool, &id).await?;                      // 运行时用 ?

// ❌ 运行时使用 unwrap
let user = db::get_user(pool, &id).await.unwrap(); // 可能 panic
```

---

## 3. 错误处理

### 3.1 统一错误枚举

- [ ] 是否定义了统一的 `AppError` 枚举，使用 `thiserror` 派生
- [ ] 每个变体是否正确映射到 HTTP 状态码（通过 `IntoResponse` 实现）
- [ ] `Internal` 变体是否隐藏内部细节，只返回通用消息给客户端
- [ ] 新增错误场景是否有对应变体，而非滥用 `Internal(String)` 兜底

```rust
// ✅ 结构化错误枚举
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".into()),
            Self::Internal(msg) => {
                tracing::error!("Internal error: {msg}"); // 记录完整错误
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into()) // 返回通用消息
            }
            Self::Sqlx(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
        };
        (status, Json(json!({ "message": message }))).into_response()
    }
}
```

### 3.2 错误传播

- [ ] handler 返回类型为 `Result<T, AppError>`
- [ ] 领域错误类型（`CryptoError`, `DockerError` 等）是否实现了 `From<XxxError> for AppError`
- [ ] `map_err` 闭包是否提供有意义的上下文信息
- [ ] DB 层错误是否通过 `#[from]` 自动转换

```rust
// ✅ 链式错误传播
async fn get_item_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthUser,
) -> Result<Json<ItemResponse>, AppError> {
    let item = db::get_item(&state.pool, &id, &auth.user_id)
        .await?  // sqlx::Error 自动转为 AppError::Sqlx
        .ok_or_else(|| AppError::NotFound("Item not found".into()))?;
    Ok(Json(item.into()))
}
```

### 3.3 WebSocket 错误处理

- [ ] WS 消息解析失败时 `continue` 而非断开连接
- [ ] DB 操作失败时通过 WS 发送结构化错误消息
- [ ] 容器初始化失败时正确清理状态（停止容器、移除连接、通知客户端）

```rust
// ✅ WS 消息解析容错
while let Some(Ok(msg)) = receiver.next().await {
    let text = match msg.to_text() {
        Ok(t) => t,
        Err(_) => continue, // 跳过非文本消息，不断开连接
    };
    let parsed: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Invalid WS message: {e}");
            continue; // 跳过无法解析的消息
        }
    };
    // 处理消息...
}
```

---

## 4. 安全性

### 4.1 认证与授权

- [ ] 受保护路由是否使用认证 extractor（如 `AuthUser`）
- [ ] 管理员路由是否使用额外的权限 extractor（如 `AdminOnly`）
- [ ] 数据查询是否始终包含 `user_id` 条件，防止越权访问
- [ ] WebSocket 连接是否在 HTTP upgrade 前验证 token
- [ ] 容器级 token 是否 scope 限定到单个资源

```rust
// ✅ 数据隔离：查询始终带 user_id
sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ? AND user_id = ?")
    .bind(id)
    .bind(user_id)  // 确保只能访问自己的数据
    .fetch_optional(pool)
    .await

// ❌ 缺少 user_id 条件，任何用户可访问任何数据
sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
    .bind(id)
    .fetch_optional(pool)
    .await
```

### 4.2 密码与密钥安全

- [ ] 密码哈希使用 Argon2id，每次生成随机 salt
- [ ] 密码验证在 `spawn_blocking` 中执行（避免阻塞 Tokio 运行时）
- [ ] 加密存储使用 AES-256-GCM，每次加密使用随机 12 字节 nonce
- [ ] nonce 不可重用（随机生成而非计数器，除非有严格的计数器管理）
- [ ] JWT secret 和 encryption key 从环境变量读取，不硬编码
- [ ] refresh token 存储 SHA-256 哈希而非明文

```rust
// ✅ 密码验证放在 spawn_blocking 中
let is_valid = tokio::task::spawn_blocking(move || {
    let parsed_hash = PasswordHash::new(&password_hash)?;
    Argon2::default().verify_password(password.as_bytes(), &parsed_hash)
}).await??;

// ❌ 直接在 async 上下文中做 CPU 密集型操作
let parsed_hash = PasswordHash::new(&password_hash)?;
Argon2::default().verify_password(password.as_bytes(), &parsed_hash)?;
```

### 4.3 Token 管理

- [ ] access token 设置合理的 TTL（建议 15min ~ 2h）
- [ ] refresh token 实现 rotation（消费旧 token + 发放新 token 在同一事务中）
- [ ] logout 时删除 refresh token 并清除 cookie
- [ ] cookie 设置 `HttpOnly`、`SameSite=Lax`，生产环境启用 `Secure`
- [ ] JWT 验证时显式指定允许的算法，拒绝 `none` 算法

```rust
// ✅ JWT 验证显式指定算法
let mut validation = Validation::new(Algorithm::HS256);
validation.set_required_spec_claims(&["exp", "iat", "sub"]);
let token_data = decode::<Claims>(&token, &decoding_key, &validation)?;

// ❌ 依赖默认 validation（应显式声明算法和必需 claims）
let token_data = decode::<Claims>(&token, &decoding_key, &Validation::default())?;
```

### 4.4 输入校验与注入防护

- [ ] 请求结构体使用 `validator` 的 `#[validate]` 宏（长度、格式、范围）
- [ ] handler 在处理前调用 `req.validate()?`
- [ ] SQL 查询全部使用参数化绑定（`?` 占位符 + `.bind()`），无字符串拼接
- [ ] 用户输入经过 trim 和空值检查

```rust
// ✅ 参数化查询
sqlx::query("DELETE FROM items WHERE id = ? AND user_id = ?")
    .bind(&id)
    .bind(&user_id)
    .execute(pool).await?;

// ❌ 字符串拼接 SQL（SQL 注入风险）
sqlx::query(&format!("DELETE FROM items WHERE id = '{id}'"))
    .execute(pool).await?;
```

### 4.5 限流与 CORS

- [ ] 认证路由（login/register/refresh）配置了限流中间件
- [ ] 限流 key 支持反向代理场景（X-Forwarded-For）
- [ ] CORS allowed origins 从环境变量读取，生产环境不使用 `allow_origin(Any)`
- [ ] WebSocket 连接独立检查 Origin header

### 4.6 依赖安全与 unsafe 策略

- [ ] 定期运行 `cargo audit` 检查已知漏洞
- [ ] 使用 `cargo deny` 检查许可证兼容性和重复依赖
- [ ] 新增依赖前确认其维护状态、流行度和安全记录
- [ ] crate 级别声明 `#![forbid(unsafe_code)]`（除非确实需要 unsafe）
- [ ] 如必须使用 unsafe，需有详细的 `// SAFETY:` 注释说明不变量

```rust
// ✅ 在 lib.rs / main.rs 顶部禁止 unsafe
#![forbid(unsafe_code)]

// ✅ 如果确实需要 unsafe，必须注释安全不变量
// SAFETY: `ptr` is guaranteed to be non-null and properly aligned
// because it was just allocated by `Vec::as_mut_ptr()`.
unsafe { *ptr = value; }
```

### 4.7 路径遍历防护

- [ ] 文件上传/下载路径是否做了规范化和白名单校验
- [ ] 用户提供的文件名是否过滤了 `..`、`/`、`\` 等路径分隔符
- [ ] 文件操作是否限定在预期的目录范围内

```rust
// ✅ 路径安全校验
use std::path::Path;

fn safe_file_path(base_dir: &Path, user_filename: &str) -> Result<PathBuf, AppError> {
    let sanitized = Path::new(user_filename)
        .file_name()  // 只取文件名部分，去除路径
        .ok_or_else(|| AppError::BadRequest("Invalid filename".into()))?;
    let full_path = base_dir.join(sanitized);
    // 确认解析后的路径仍在 base_dir 内
    if !full_path.starts_with(base_dir) {
        return Err(AppError::BadRequest("Path traversal detected".into()));
    }
    Ok(full_path)
}
```

---

## 5. 异步模式与并发

### 5.1 Tokio 运行时使用

- [ ] CPU 密集型操作（密码哈希、加密）使用 `tokio::task::spawn_blocking`
- [ ] 后台任务使用 `tokio::spawn`（如容器启动、定时清理）
- [ ] `tokio::select!` 正确处理多个 future 的竞争
- [ ] graceful shutdown 正确实现（signal handler + 资源清理）
- [ ] 不在 `Drop` 中调用异步代码

### 5.1.1 Cancellation Safety

- [ ] `tokio::select!` 中的 future 是否 cancellation safe
- [ ] 被取消的 future 是否可能留下不一致的状态（如写了一半的数据、未释放的资源）
- [ ] 关键操作是否使用 `tokio::pin!` + loop 模式避免重复执行

```rust
// ✅ Cancellation safe 的 select! 用法
let mut recv_fut = std::pin::pin!(receiver.next());
loop {
    tokio::select! {
        msg = &mut recv_fut => {
            match msg {
                Some(Ok(m)) => { /* 处理消息 */ },
                _ => break,
            }
            recv_fut.set(receiver.next()); // 重新设置 future
        }
        _ = shutdown.recv() => {
            tracing::info!("Shutting down connection");
            break;
        }
    }
}

// ❌ 不安全：如果 do_work 在 select! 中被取消，可能留下不一致状态
tokio::select! {
    result = do_work_that_modifies_state() => { /* ... */ }
    _ = timeout => { /* do_work 被取消，状态可能不一致 */ }
}
```

### 5.2 锁与并发数据结构

- [ ] 共享状态使用 `Arc<T>` 包装
- [ ] 读多写少场景使用 `RwLock`，写多场景使用 `Mutex`
- [ ] 需要细粒度锁的场景使用 `DashMap`
- [ ] 锁的持有时间尽可能短，不在持锁期间做 I/O
- [ ] `std::sync::Mutex` 不跨 `.await` 持有（否则用 `tokio::sync::Mutex`）

```rust
// ✅ 短暂持锁，不跨 await
let sender = {
    let map = state.connections.read().await;
    map.get(&id).cloned() // clone sender，立即释放锁
};
if let Some(sender) = sender {
    sender.send(msg).await?; // 在锁外做 I/O
}

// ❌ 持锁跨 await（可能死锁或阻塞运行时）
let map = state.connections.read().await;
let sender = map.get(&id).unwrap();
sender.send(msg).await?; // 仍然持有 RwLock 读锁
```

### 5.3 Channel 使用

- [ ] `mpsc::channel` 容量设置合理（考虑突发流量）
- [ ] `try_send` 失败时有降级处理（日志 + 丢弃），而非 panic
- [ ] channel 关闭时正确清理关联资源
- [ ] 使用 bounded channel 防止内存无限增长

### 5.4 TOCTOU 防护

- [ ] 检查-操作序列是否有竞态风险
- [ ] 需要原子性的操作是否使用 per-key 锁（如 `DashMap<Key, Arc<Mutex<()>>>`）

```rust
// ✅ per-key 锁防止 TOCTOU
let lock = start_locks
    .entry(conversation_id.clone())
    .or_insert_with(|| Arc::new(Mutex::new(())))
    .clone();
let _guard = lock.lock().await;
// 在锁内执行 check-then-act 操作
if !is_container_running(&conversation_id).await {
    start_container(&conversation_id).await?;
}
```

---

## 6. 数据库

### 6.1 SQLx 使用规范

- [ ] 使用 `sqlx::query_as::<_, T>` 做类型安全映射
- [ ] `RETURNING` 子句用于 INSERT/UPDATE 后返回完整行，避免额外查询
- [ ] 根据预期结果数量选择正确的 fetch 方法：
  - `fetch_one` — 确定有且仅有一行
  - `fetch_optional` — 可能为空（返回 `Option<T>`）
  - `fetch_all` — 多行结果
  - `execute` — 不需要返回行（DELETE、纯 UPDATE）

```rust
// ✅ INSERT + RETURNING 避免额外查询
let item = sqlx::query_as::<_, Item>(
    "INSERT INTO items (id, name, user_id) VALUES (?, ?, ?) RETURNING *"
)
.bind(&id).bind(&name).bind(&user_id)
.fetch_one(pool).await?;

// ❌ INSERT 后再 SELECT（多一次 round trip）
sqlx::query("INSERT INTO items (id, name, user_id) VALUES (?, ?, ?)")
    .bind(&id).bind(&name).bind(&user_id)
    .execute(pool).await?;
let item = sqlx::query_as::<_, Item>("SELECT * FROM items WHERE id = ?")
    .bind(&id).fetch_one(pool).await?;
```

### 6.2 事务

- [ ] 多步写操作使用事务（`pool.begin()` + `tx.commit()`）
- [ ] 事务内的函数接受 `&mut Transaction` 参数
- [ ] 事务失败时自动回滚（drop Transaction 即回滚）
- [ ] 关联操作（如 token rotation：DELETE + INSERT）在同一事务中完成

```rust
// ✅ 事务保证原子性
let mut tx = pool.begin().await?;
db::delete_old_token(&mut *tx, &old_hash).await?;
db::insert_new_token(&mut *tx, &new_hash, user_id).await?;
tx.commit().await?; // 两步操作要么全成功，要么全回滚
```

### 6.3 SQLite 特定注意事项

- [ ] WAL 模式在初始化时启用（`PRAGMA journal_mode=WAL`）
- [ ] foreign keys 启用（`PRAGMA foreign_keys=ON`）
- [ ] 连接池大小合理（SQLite 建议 `max_connections` 较小，如 5）
- [ ] 并发写入考虑 SQLite 的单写者限制（写操作可能需要重试）

### 6.4 Migration

- [ ] migration 文件使用时间戳前缀命名（`YYYYMMDDHHMMSS_description.sql`）
- [ ] migration 尽量幂等（`IF NOT EXISTS`、`IF EXISTS`）
- [ ] schema 变更向后兼容（新增列使用 `DEFAULT`，不删除正在使用的列）
- [ ] `sqlx::migrate!()` 在应用启动时自动执行

### 6.5 查询性能

- [ ] WHERE 条件中的列有索引（特别是 `user_id`、外键列、`share_token` 等）
- [ ] 列表查询支持分页（`LIMIT` + `OFFSET` 或 cursor-based）
- [ ] 排序有稳定的 tiebreaker（如 `ORDER BY updated_at DESC, id DESC`）
- [ ] 避免 N+1 查询，使用批量查询（`WHERE id IN (?, ?, ...)`）

---

## 7. API 设计

### 7.1 路由组织

- [ ] 路由按资源分组，使用 `Router::new().nest()` 组织
- [ ] RESTful 语义正确：GET 读取、POST 创建、PUT/PATCH 更新、DELETE 删除
- [ ] 状态码正确：201 Created、204 No Content、404 Not Found、409 Conflict
- [ ] 每个模块导出 `router() -> Router<Arc<AppState>>` 函数

```rust
// ✅ 路由组织
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/items", get(list_items).post(create_item))
        .route("/items/{id}", get(get_item).put(update_item).delete(delete_item))
}

// 主路由组合
let app = Router::new()
    .nest("/api/items", items::router())
    .nest("/api/users", users::router())
    .layer(cors_layer)
    .layer(trace_layer)
    .with_state(state);
```

### 7.1.1 中间件顺序

- [ ] `layer()` 顺序正确（后添加的先执行，即"洋葱模型"）
- [ ] `HandleErrorLayer` 在 `TimeoutLayer` 之上（否则超时错误无法转为 HTTP 响应）
- [ ] 认证中间件使用 `route_layer`（仅对匹配的路由生效），避免 404 变成 401
- [ ] 限流中间件仅应用于需要限流的路由组

```rust
// ✅ 中间件顺序：后添加的先执行
let app = Router::new()
    .nest("/api/auth", auth::router()
        .layer(GovernorLayer { config: rate_limit_config })) // 仅限流 auth 路由
    .nest("/api/items", items::router())
    .layer(TraceLayer::new_for_http())  // 内层：记录所有请求
    .layer(CorsLayer::permissive())     // 外层（最先执行）：处理 CORS preflight
    .with_state(state);
```

### 7.2 请求处理

- [ ] Extractor 顺序正确：`State` 和 `Path` 在前，`Json`（消费 body）在最后
- [ ] 请求体在处理前调用 `validate()`
- [ ] 分页参数有默认值和上限

```rust
// ✅ Extractor 顺序 + 校验
async fn create_item(
    State(state): State<Arc<AppState>>,  // 1. State
    auth: AuthUser,                       // 2. 认证（从 header 提取）
    Json(req): Json<CreateItemRequest>,   // 3. Body（最后，消费 request body）
) -> Result<impl IntoResponse, AppError> {
    req.validate().map_err(|e| AppError::BadRequest(e.to_string()))?;
    // ...
}
```

### 7.3 响应格式

- [ ] 错误响应统一为 `{"message": "xxx"}` 格式
- [ ] 列表响应包含 `total` 字段用于分页
- [ ] 敏感字段（password_hash、encrypted_key）从响应中排除
- [ ] 使用 `#[serde(skip_serializing)]` 或独立的 Response 结构体排除字段

---

## 8. WebSocket

### 8.1 消息协议

- [ ] 消息类型使用 `#[serde(tag = "type")]` tagged enum
- [ ] 未知消息类型使用 `#[serde(other)]` 优雅降级（forward/ignore）
- [ ] 转发消息保留原始 JSON 结构

```rust
// ✅ Tagged enum + 未知类型降级
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContainerMessage {
    Ready,
    Complete { message_id: String },
    Error { code: String, message: String },
    #[serde(other)]
    Forward, // 未知类型直接转发，不中断连接
}
```

### 8.2 连接状态管理

- [ ] 客户端连接按 `(user_id, conversation_id)` 组织
- [ ] 容器连接使用 generation 计数器防止旧连接清理新连接
- [ ] 连接断开时清理注册信息和关联资源
- [ ] 发送任务在连接关闭时 abort

### 8.3 容器通信

- [ ] init payload 包含所有必要配置
- [ ] 大 payload 有 warn 级别日志
- [ ] 敏感数据（API key）解密后发送，失败时正确清理
- [ ] 消息排队机制处理容器启动期间的消息

---

## 9. Docker 容器管理

### 9.1 容器生命周期

- [ ] 容器创建时设置资源限制（memory、CPU）
- [ ] 容器启动有 TOCTOU 防护（per-conversation mutex）
- [ ] 容器停止先 stop（带超时）再 force remove
- [ ] 同名容器在创建前先 force remove（处理崩溃残留）
- [ ] 容器操作处理"已停止/已删除"的幂等场景

```rust
// ✅ 幂等的容器清理
async fn remove_container(docker: &Docker, name: &str) {
    let opts = RemoveContainerOptions { force: true, ..Default::default() };
    match docker.remove_container(name, Some(opts)).await {
        Ok(_) => tracing::info!("Removed container {name}"),
        Err(BollardError::DockerResponseServerError { status_code: 404, .. }) => {
            // 容器不存在，忽略（幂等）
        }
        Err(e) => tracing::error!("Failed to remove container {name}: {e}"),
    }
}
```

### 9.2 Idle 清理

- [ ] 定时任务定期检查 idle 容器（如每 30s）
- [ ] 清理时先从内存注册表移除，再操作 Docker API
- [ ] 活跃容器通过 `touch_activity` 刷新时间戳
- [ ] 并行停止多个 idle 容器（`futures::future::join_all`）

### 9.3 Graceful Shutdown

- [ ] shutdown 信号触发所有容器停止
- [ ] shutdown 等待容器清理完成后再退出
- [ ] 使用 `tokio::signal` 监听 SIGTERM/SIGINT

---

## 10. 测试

### 10.1 单元测试

- [ ] 每个模块有 `#[cfg(test)] mod tests` 内联测试
- [ ] 纯函数有独立的单元测试
- [ ] 错误路径有测试覆盖（无效输入、权限不足、资源不存在）
- [ ] 异步测试使用 `#[tokio::test]`

### 10.2 集成测试

- [ ] 使用 in-memory SQLite（`sqlite::memory:`）做测试隔离
- [ ] 通过 `tower::ServiceExt::oneshot` 直接调用 router，不启动真实 HTTP server
- [ ] 测试辅助函数提取为共享 helper
- [ ] 认证测试覆盖 token 过期、错误 token、缺失 token 场景

```rust
// ✅ 集成测试模式
#[tokio::test]
async fn test_create_item_requires_auth() {
    let state = test_state().await; // in-memory SQLite + test config
    let app = items::router().with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/items")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

### 10.3 测试质量

- [ ] 遵循 Arrange-Act-Assert 模式
- [ ] 测试名称描述行为（`test_delete_other_users_item_returns_forbidden`）
- [ ] 边界条件覆盖（空字符串、None、重复操作、并发场景）
- [ ] 新功能先写测试（TDD）

---

## 11. 性能

### 11.1 内存与分配

- [ ] 大集合使用 `Vec::with_capacity` 预分配
- [ ] 避免不必要的 `.clone()` 和 `.to_string()`
- [ ] 函数参数优先使用 `&str` 而非 `String`
- [ ] 大 JSON payload 有大小检查

### 11.1.1 反序列化安全

- [ ] JSON 反序列化设置了 body size limit（`DefaultBodyLimit`），防止超大 payload 耗尽内存
- [ ] 深度嵌套的 JSON 不会导致栈溢出（serde_json 默认有递归限制，但自定义 Deserialize 需注意）
- [ ] `#[serde(deny_unknown_fields)]` 用于严格模式的请求体（可选，视场景而定）
- [ ] 枚举反序列化使用 `#[serde(rename_all = "snake_case")]` 保持一致性

### 11.2 I/O 效率

- [ ] 批量 Docker 操作并行执行（`join_all`）
- [ ] DB 批量查询使用 `IN` 子句而非循环单条查询
- [ ] 文件上传设置 `DefaultBodyLimit`
- [ ] 流式处理大文件（`ReaderStream`），不一次性加载到内存

### 11.3 连接管理

- [ ] SQLite 连接池大小合理
- [ ] WS channel 容量足够处理突发流量
- [ ] Docker client 复用（只初始化一次）
- [ ] HTTP client（reqwest）复用连接池

---

## 12. 可观测性

### 12.1 日志规范

- [ ] 使用 `tracing` 宏（`info!`, `error!`, `warn!`, `debug!`）而非 `println!`
- [ ] 错误日志包含结构化字段（`conversation_id = %id, error = %e`）
- [ ] 敏感信息（API key、密码、token）从日志中排除
- [ ] `RUST_LOG` 环境变量支持运行时调整日志级别

```rust
// ✅ 结构化日志
tracing::info!(
    conversation_id = %id,
    container_name = %name,
    "Container started successfully"
);

tracing::error!(
    user_id = %auth.user_id,
    error = %e,
    "Failed to create item"  // 不包含敏感数据
);

// ❌ 非结构化日志 + 泄露敏感信息
println!("Error for user {} with key {}: {}", user_id, api_key, e);
```

### 12.2 关键事件追踪

- [ ] 容器启动/停止有 info 级别日志
- [ ] WS 连接/断开有日志
- [ ] idle cleanup 记录被清理的容器
- [ ] 大 payload 有 warn 级别日志
- [ ] HTTP 请求通过 `TraceLayer` 自动记录

---

## 13. 配置管理

### 13.1 环境变量与 Config

- [ ] 所有配置通过环境变量注入，不硬编码
- [ ] Config 结构体使用 `#[derive(Deserialize)]` + `envy::from_env()` 类型安全反序列化
- [ ] 必填字段无默认值（缺失时启动失败并给出明确错误）
- [ ] 可选字段使用 `#[serde(default = "default_fn")]` 提供合理默认值
- [ ] 敏感配置（JWT_SECRET、ENCRYPTION_KEY）不出现在日志或错误消息中

```rust
// ✅ 类型安全的配置
#[derive(Deserialize)]
pub struct Config {
    pub jwt_secret: String,           // 必填，缺失时启动失败
    pub encryption_key: String,       // 必填
    #[serde(default = "default_port")]
    pub port: u16,                    // 可选，有默认值
    #[serde(default = "default_idle_timeout")]
    pub container_idle_timeout: u64,  // 可选，有默认值
}

fn default_port() -> u16 { 3000 }
fn default_idle_timeout() -> u64 { 600 }
```

### 13.2 Feature Flags 与条件编译

- [ ] 开发/生产环境差异通过环境变量控制，而非 `#[cfg(debug_assertions)]`
- [ ] 测试专用代码使用 `#[cfg(test)]` 隔离
- [ ] `lib.rs` 导出所有模块以支持集成测试（`backend/tests/`）

---

## 附录：Review Checklist 速查表

| 类别 | 核心检查项 |
|------|-----------|
| 代码质量 | 分层清晰、类型分离、命名规范、无 unwrap |
| 错误处理 | 统一 AppError、? 传播、不泄露内部错误 |
| 安全性 | AuthUser extractor、user_id 隔离、参数化 SQL、限流、cargo audit、路径遍历防护 |
| 异步并发 | spawn_blocking、短持锁、bounded channel、TOCTOU 防护、cancellation safety |
| 数据库 | query_as 类型映射、事务原子性、RETURNING、索引 |
| API 设计 | RESTful 语义、Extractor 顺序、validate()、分页、中间件顺序 |
| WebSocket | tagged enum、generation 计数器、连接清理 |
| Docker | 资源限制、幂等清理、graceful shutdown |
| 测试 | in-memory SQLite、oneshot、AAA 模式、TDD |
| 性能 | with_capacity、批量查询、流式处理、连接复用、反序列化安全 |
| 可观测性 | tracing 结构化日志、敏感信息排除、关键事件追踪 |
| 配置管理 | 类型安全 Config、环境变量注入、敏感配置不泄露 |
