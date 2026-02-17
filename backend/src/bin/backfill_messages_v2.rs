use claude_chat_backend::backfill::backfill_messages_v2;
use claude_chat_backend::config::Config;
use claude_chat_backend::db;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env();
    let pool = db::init_db(&config.database_url).await;

    match backfill_messages_v2(&pool).await {
        Ok(stats) => {
            tracing::info!(
                scanned = stats.scanned,
                inserted = stats.inserted,
                skipped_existing = stats.skipped_existing,
                failed = stats.failed,
                "messages_v2 backfill completed"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "messages_v2 backfill failed");
            std::process::exit(1);
        }
    }
}
