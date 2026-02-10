pub mod conversations;
pub mod mcp_servers;
pub mod messages;
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

    tracing::info!("Database migrations applied successfully");
}
