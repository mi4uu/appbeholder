use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Sse},
};
use axum_extra::extract::cookie::SignedCookieJar;
use askama::Template;
use serde::Deserialize;
use std::convert::Infallible;
use uuid::Uuid;

use crate::AppState;
use crate::db;
use crate::auth;

// --- View models ---

pub struct ProjectInfo {
    pub slug: String,
    pub name: String,
    pub selected: bool,
}

pub struct HostInfo {
    pub id: String,
    pub hostname: String,
}

// --- Templates ---

#[derive(Template)]
#[template(path = "layout.html")]
struct LayoutTemplate {
    title: String,
    projects: Vec<ProjectInfo>,
    has_project: bool,
    current_slug: String,
    content: String,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error_message: String,
    has_error: bool,
}

#[derive(Template)]
#[template(path = "logs.html")]
struct LogsTemplate {
    project_slug: String,
    project_name: String,
    hosts: Vec<HostInfo>,
    logs_html: String,
}

// --- Handlers ---

fn build_projects(pool_projects: &[(Uuid, String, String)], current: Option<&str>) -> Vec<ProjectInfo> {
    pool_projects.iter().map(|(_, name, slug)| ProjectInfo {
        slug: slug.clone(),
        name: name.clone(),
        selected: current == Some(slug.as_str()),
    }).collect()
}

pub async fn index(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();

    if let Some((_id, _name, slug)) = pool_projects.first() {
        Redirect::to(&format!("/projects/{}/logs", slug)).into_response()
    } else {
        let content = r#"<div class="hero min-h-screen">
            <div class="hero-content text-center">
                <div>
                    <h1 class="text-4xl font-bold">Welcome to App Beholder</h1>
                    <p class="py-6">No projects yet. Send your first log to create one automatically.</p>
                    <pre class="bg-base-200 p-4 rounded text-left text-sm"><code>curl -X POST http://localhost:8080/api/v1/logs \
  -H "Content-Type: application/json" \
  -H "X-Project-Slug: my-app" \
  -d '{"level":"info","message":"Hello from App Beholder!"}'</code></pre>
                </div>
            </div>
        </div>"#;

        let template = LayoutTemplate {
            title: "App Beholder".to_string(),
            projects: build_projects(&pool_projects, None),
            has_project: false,
            current_slug: String::new(),
            content: content.to_string(),
        };
        Html(template.render().unwrap_or_default()).into_response()
    }
}

pub async fn login_page() -> impl IntoResponse {
    let template = LoginTemplate {
        error_message: String::new(),
        has_error: false,
    };
    Html(template.render().unwrap_or_default())
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    axum::Form(form): axum::Form<LoginForm>,
) -> impl IntoResponse {
    if let Some(ref expected) = state.password {
        if form.password == *expected {
            let jar = jar.add(auth::create_session_cookie());
            return (jar, Redirect::to("/")).into_response();
        }
    }

    let template = LoginTemplate {
        error_message: "Invalid password".to_string(),
        has_error: true,
    };
    (StatusCode::UNAUTHORIZED, Html(template.render().unwrap_or_default())).into_response()
}

pub async fn logs_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();

    let project_name = pool_projects
        .iter()
        .find(|(_, _, s)| s == &slug)
        .map(|(_, n, _)| n.clone())
        .unwrap_or_else(|| slug.clone());

    let project_id = pool_projects
        .iter()
        .find(|(_, _, s)| s == &slug)
        .map(|(id, _, _)| *id);

    let hosts_raw = if let Some(pid) = project_id {
        db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default()
    } else {
        vec![]
    };

    let hosts: Vec<HostInfo> = hosts_raw.iter().map(|(id, hostname)| HostInfo {
        id: id.to_string(),
        hostname: hostname.clone(),
    }).collect();

    // Load recent logs
    let logs_html = if let Some(pid) = project_id {
        let query = db::logs::LogQuery {
            project_id: pid,
            level: None,
            host_id: None,
            search: None,
            limit: 100,
            before: None,
        };
        let logs = db::logs::query_logs(&state.pool, &query).await.unwrap_or_default();
        let host_map: std::collections::HashMap<Uuid, String> = hosts_raw.iter().cloned().collect();
        logs.iter().rev().map(|l| {
            let hostname = host_map.get(&l.host_id).map(|s| s.as_str()).unwrap_or("unknown");
            crate::api::logs::render_log_row(l, hostname)
        }).collect::<Vec<_>>().join("\n")
    } else {
        String::new()
    };

    let logs_template = LogsTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
        logs_html,
    };

    let content = logs_template.render().unwrap_or_default();

    let template = LayoutTemplate {
        title: format!("Logs - {}", project_name),
        projects: build_projects(&pool_projects, Some(&slug)),
        has_project: true,
        current_slug: slug,
        content,
    };

    Html(template.render().unwrap_or_default())
}

#[derive(Deserialize)]
pub struct LogsQuery {
    level: Option<String>,
    host_id: Option<Uuid>,
    search: Option<String>,
}

pub async fn logs_data(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<LogsQuery>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_id = pool_projects
        .iter()
        .find(|(_, _, s)| s == &slug)
        .map(|(id, _, _)| *id);

    let Some(pid) = project_id else {
        return Html(String::new());
    };

    let hosts = db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default();
    let host_map: std::collections::HashMap<Uuid, String> = hosts.into_iter().collect();

    let query = db::logs::LogQuery {
        project_id: pid,
        level: params.level,
        host_id: params.host_id,
        search: params.search,
        limit: 100,
        before: None,
    };

    let logs = db::logs::query_logs(&state.pool, &query).await.unwrap_or_default();
    let html: String = logs.iter().rev().map(|l| {
        let hostname = host_map.get(&l.host_id).map(|s| s.as_str()).unwrap_or("unknown");
        crate::api::logs::render_log_row(l, hostname)
    }).collect::<Vec<_>>().join("\n");

    Html(html)
}

pub async fn sse_logs(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let mut rx = state.sse.subscribe_logs(&slug).await;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    yield Ok(axum::response::sse::Event::default()
                        .event("log")
                        .data(event.html));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE client lagged by {} messages", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
    )
}
