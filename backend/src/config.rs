use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub encryption_key: String,
    pub host: String,
    pub port: u16,
    pub container_image: String,
    pub container_idle_timeout_secs: u64,
    pub internal_ws_port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:data/claude-chat.db?mode=rwc".into()),
            jwt_secret: env::var("JWT_SECRET").expect("JWT_SECRET must be set"),
            encryption_key: env::var("ENCRYPTION_KEY").expect("ENCRYPTION_KEY must be set"),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .expect("PORT must be a number"),
            container_image: env::var("CONTAINER_IMAGE")
                .unwrap_or_else(|_| "claude-chat-agent:latest".into()),
            container_idle_timeout_secs: env::var("CONTAINER_IDLE_TIMEOUT")
                .unwrap_or_else(|_| "600".into())
                .parse()
                .expect("CONTAINER_IDLE_TIMEOUT must be a number"),
            internal_ws_port: env::var("INTERNAL_WS_PORT")
                .unwrap_or_else(|_| "3001".into())
                .parse()
                .expect("INTERNAL_WS_PORT must be a number"),
        }
    }
}
