pub mod conversations;
pub mod mcp_servers;
pub mod messages;
pub mod messages_v2;
pub mod presets;
pub mod providers;
pub mod refresh_tokens;
pub mod users;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

/// Initialize the SQLite connection pool, enable WAL mode and foreign keys,
/// and run migrations via sqlx::migrate!().
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

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    pool
}
