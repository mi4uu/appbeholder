use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_url")]
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_logs_days")]
    pub logs_days: u32,
    #[serde(default = "default_traces_days")]
    pub traces_days: u32,
    #[serde(default = "default_metrics_days")]
    pub metrics_days: u32,
    #[serde(default = "default_errors_days")]
    pub errors_days: u32,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8080 }
fn default_db_url() -> String { "postgres://localhost/appbeholder".to_string() }
fn default_logs_days() -> u32 { 7 }
fn default_traces_days() -> u32 { 30 }
fn default_metrics_days() -> u32 { 90 }
fn default_errors_days() -> u32 { 30 }

impl Default for ServerConfig {
    fn default() -> Self {
        Self { host: default_host(), port: default_port() }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self { url: default_db_url() }
    }
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            logs_days: default_logs_days(),
            traces_days: default_traces_days(),
            metrics_days: default_metrics_days(),
            errors_days: default_errors_days(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = std::path::Path::new("config.toml");
        if path.exists() {
            let content = std::fs::read_to_string(path).expect("Failed to read config.toml");
            toml::from_str(&content).expect("Failed to parse config.toml")
        } else {
            tracing::info!("No config.toml found, using defaults");
            toml::from_str("").unwrap()
        }
    }
}
