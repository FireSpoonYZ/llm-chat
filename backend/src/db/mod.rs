pub mod conversations;
pub mod mcp_servers;
pub mod messages;
pub mod presets;
pub mod providers;
pub mod refresh_tokens;
pub mod users;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Initialize the SQLite connection pool, enable WAL mode and foreign keys,
/// and run the initial migration.
pub async fn init_db(database_url: &str) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(database_url)
        .expect("Invalid DATABASE_URL")
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("Failed to connect to SQLite database");

    run_migrations(&pool).await;

    pool
}

async fn run_migrations(pool: &SqlitePool) {
    let migration_sql = include_str!("../../../migrations/001_initial.sql");

    // Split on semicolons and execute each statement individually.
    // SQLite's execute does not support multiple statements in one call.
    for statement in migration_sql.split(';') {
        let trimmed: &str = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed)
            .execute(pool)
            .await
            .unwrap_or_else(|e| {
                panic!("Migration statement failed: {e}\nSQL: {trimmed}");
            });
    }

    // Run incremental migrations (may fail if already applied)
    let migration_002 = include_str!("../../../migrations/002_provider_models.sql");
    for statement in migration_002.split(';') {
        let trimmed: &str = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Ignore errors (e.g. column already exists)
        let _ = sqlx::query(trimmed).execute(pool).await;
    }

    // Migration 003: add name column and recreate table without UNIQUE(user_id, provider)
    // Only run if name column doesn't exist yet to avoid resetting custom names
    let has_name_col = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('user_providers') WHERE name = 'name'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if has_name_col == 0 {
        let migration_003 = include_str!("../../../migrations/003_provider_name.sql");
        for statement in migration_003.split(';') {
            let trimmed: &str = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed)
                .execute(pool)
                .await
                .unwrap_or_else(|e| {
                    panic!("Migration 003 failed: {e}\nSQL: {trimmed}");
                });
        }
    }

    // Migration 004: user_presets table
    let has_presets_table = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_presets'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if has_presets_table == 0 {
        let migration_004 = include_str!("../../../migrations/004_user_presets.sql");
        for statement in migration_004.split(';') {
            let trimmed: &str = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed)
                .execute(pool)
                .await
                .unwrap_or_else(|e| {
                    panic!("Migration 004 failed: {e}\nSQL: {trimmed}");
                });
        }
    }

    // Migration 005: add deep_thinking column to conversations
    let has_deep_thinking_col = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'deep_thinking'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if has_deep_thinking_col == 0 {
        let migration_005 = include_str!("../../../migrations/005_conversation_deep_thinking.sql");
        for statement in migration_005.split(';') {
            let trimmed: &str = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed)
                .execute(pool)
                .await
                .unwrap_or_else(|e| {
                    panic!("Migration 005 failed: {e}\nSQL: {trimmed}");
                });
        }
    }

    tracing::info!("Database migrations applied successfully");
}
