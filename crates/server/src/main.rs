mod api;
mod auth;
mod config;
mod db;
mod sse;
mod web;

use deadpool_postgres::Pool;
use config::AppConfig;
use sse::channels::SseChannels;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub sse: SseChannels,
    pub password: Option<String>,
    pub cookie_key: axum_extra::extract::cookie::Key,
}

impl axum::extract::FromRef<AppState> for axum_extra::extract::cookie::Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appbeholder=debug,tower_http=debug".into()),
        )
        .init();

    let config = AppConfig::load();
    tracing::info!("App Beholder starting on {}:{}", config.server.host, config.server.port);

    let pool = db::create_pool(&config.database);
    db::migrations::run_migrations(&pool).await.expect("Failed to run migrations");

    let sse = SseChannels::new();

    // Load password from .password file if it exists
    let password = std::fs::read_to_string(".password")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if password.is_some() {
        tracing::info!("Password authentication enabled (.password file found)");
    } else {
        tracing::info!("No authentication (no .password file)");
    }

    let state = AppState {
        pool: pool.clone(),
        sse,
        password,
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    };

    // Spawn retention task
    let retention_pool = pool.clone();
    let retention_config = config.retention;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            if let Err(e) = db::migrations::create_partitions(&retention_pool, 7).await {
                tracing::error!("Failed to create partitions: {}", e);
            }
            if let Err(e) = db::migrations::drop_old_partitions(
                &retention_pool,
                retention_config.logs_days,
                retention_config.traces_days,
                retention_config.metrics_days,
            ).await {
                tracing::error!("Failed to drop old partitions: {}", e);
            }
        }
    });

    let app = create_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("Failed to bind");
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await.expect("Server error");
}

fn create_router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};
    use axum::middleware;

    let api_routes = axum::Router::new()
        .route("/v1/logs", post(api::logs::ingest_log))
        .route("/v1/errors", post(api::errors::ingest_error));

    let sse_routes = axum::Router::new()
        .route("/logs/{slug}", get(web::sse_logs));

    let web_routes = axum::Router::new()
        .route("/", get(web::index))
        .route("/login", get(web::login_page).post(web::login_submit))
        .route("/projects/{slug}/logs", get(web::logs_page))
        .route("/api/logs/{slug}", get(web::logs_data));

    axum::Router::new()
        .nest("/api", api_routes)
        .nest("/sse", sse_routes)
        .merge(web_routes)
        .layer(middleware::from_fn_with_state(state.clone(), auth::auth_middleware))
        .with_state(state)
}
