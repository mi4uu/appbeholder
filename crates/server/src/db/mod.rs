pub mod migrations;
pub mod logs;
pub mod projects;
pub mod spans;
pub mod metrics;

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

use crate::config::DatabaseConfig;

pub fn create_pool(config: &DatabaseConfig) -> Pool {
    let mut cfg = Config::new();
    cfg.url = Some(config.url.clone());
    cfg.create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("Failed to create database pool")
}
