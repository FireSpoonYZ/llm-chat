use serde::Deserialize;

fn default_database_url() -> String {
    "sqlite:data/claude-chat.db?mode=rwc".into()
}
fn default_host() -> String {
    "0.0.0.0".into()
}
fn default_port() -> u16 {
    3000
}
fn default_container_image() -> String {
    "claude-chat-agent:latest".into()
}
fn default_container_idle_timeout() -> u64 {
    600
}
fn default_internal_ws_port() -> u16 {
    3001
}
fn default_access_token_ttl() -> u64 {
    7200
}
fn default_container_token_ttl() -> u64 {
    3600
}
fn default_refresh_token_ttl_days() -> i64 {
    30
}
fn default_cookie_secure() -> bool {
    false
}

#[derive(Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_database_url")]
    pub database_url: String,
    pub jwt_secret: String,
    pub encryption_key: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_container_image")]
    pub container_image: String,
    #[serde(
        rename = "container_idle_timeout",
        default = "default_container_idle_timeout"
    )]
    pub container_idle_timeout_secs: u64,
    #[serde(default = "default_internal_ws_port")]
    pub internal_ws_port: u16,
    pub docker_network: Option<String>,
    pub host_data_dir: Option<String>,
    pub fileserver_url: Option<String>,
    /// Comma-separated list of allowed CORS origins. If empty, allows all origins.
    pub cors_allowed_origins: Option<String>,
    /// Access token TTL in seconds (default: 7200 = 2 hours)
    #[serde(default = "default_access_token_ttl")]
    pub access_token_ttl_secs: u64,
    /// Container token TTL in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_container_token_ttl")]
    pub container_token_ttl_secs: u64,
    /// Refresh token TTL in days (default: 30)
    #[serde(default = "default_refresh_token_ttl_days")]
    pub refresh_token_ttl_days: i64,
    /// Whether auth cookies should include `Secure`.
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        envy::from_env::<Config>().expect("Failed to parse config from environment")
    }
}
