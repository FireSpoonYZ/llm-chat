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
    #[serde(rename = "container_idle_timeout", default = "default_container_idle_timeout")]
    pub container_idle_timeout_secs: u64,
    #[serde(default = "default_internal_ws_port")]
    pub internal_ws_port: u16,
    pub docker_network: Option<String>,
    pub host_data_dir: Option<String>,
    pub fileserver_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        envy::from_env::<Config>().expect("Failed to parse config from environment")
    }
}
