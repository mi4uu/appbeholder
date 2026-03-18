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
    current_page: String,
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

#[derive(Template)]
#[template(path = "placeholder.html")]
struct PlaceholderTemplate {
    page_title: String,
    page_icon: String,
    page_description: String,
    project_slug: String,
}

#[derive(Template)]
#[template(path = "projects.html")]
struct ProjectsTemplate {
    projects: Vec<ProjectDetail>,
}

pub struct ProjectDetail {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

// --- Helpers ---

fn build_projects(pool_projects: &[(Uuid, String, String)], current: Option<&str>) -> Vec<ProjectInfo> {
    pool_projects.iter().map(|(_, name, slug)| ProjectInfo {
        slug: slug.clone(),
        name: name.clone(),
        selected: current == Some(slug.as_str()),
    }).collect()
}

fn render_page(title: &str, projects: &[(Uuid, String, String)], slug: &str, page: &str, content: String) -> String {
    let template = LayoutTemplate {
        title: title.to_string(),
        projects: build_projects(projects, Some(slug)),
        has_project: true,
        current_slug: slug.to_string(),
        current_page: page.to_string(),
        content,
    };
    template.render().unwrap_or_default()
}

// --- Handlers ---

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
                    <img src="/static/logo.png" alt="App Beholder" class="w-48 mx-auto mb-6" />
                    <h1 class="text-4xl font-bold">Welcome to App Beholder</h1>
                    <p class="py-6 opacity-70">No projects yet. Send your first log to create one automatically.</p>
                    <pre class="bg-base-200 p-4 rounded text-left text-sm"><code>curl -X POST https://beholder.lipinski.work/api/v1/logs \
  -H "Content-Type: application/json" \
  -H "X-Project-Slug: my-app" \
  -d '{"level":"info","message":"Hello!"}'</code></pre>
                    <div class="divider">OR</div>
                    <a href="/projects" class="btn btn-primary">Manage Projects</a>
                </div>
            </div>
        </div>"#;

        let template = LayoutTemplate {
            title: "App Beholder".to_string(),
            projects: build_projects(&pool_projects, None),
            has_project: false,
            current_slug: String::new(),
            current_page: String::new(),
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

// --- Project management ---

pub async fn projects_page(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects_full(&state.pool).await.unwrap_or_default();

    let projects: Vec<ProjectDetail> = pool_projects.iter().map(|(id, name, slug, created)| ProjectDetail {
        id: id.to_string(),
        name: name.clone(),
        slug: slug.clone(),
        created_at: created.format("%Y-%m-%d %H:%M").to_string(),
    }).collect();

    let nav_projects = pool_projects.iter().map(|(_, name, slug, _)| (Uuid::nil(), name.clone(), slug.clone())).collect::<Vec<_>>();

    let content = (ProjectsTemplate { projects }).render().unwrap_or_default();

    let template = LayoutTemplate {
        title: "Projects - App Beholder".to_string(),
        projects: build_projects(&nav_projects, None),
        has_project: false,
        current_slug: String::new(),
        current_page: "projects".to_string(),
        content,
    };

    Html(template.render().unwrap_or_default())
}

#[derive(Deserialize)]
pub struct CreateProjectForm {
    name: String,
    slug: String,
}

pub async fn create_project(
    State(state): State<AppState>,
    axum::Form(form): axum::Form<CreateProjectForm>,
) -> impl IntoResponse {
    let slug = form.slug.trim().to_lowercase().replace(' ', "-");
    let name = form.name.trim().to_string();

    if !slug.is_empty() && !name.is_empty() {
        let _ = db::projects::create_project(&state.pool, &name, &slug).await;
    }

    Redirect::to("/projects")
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Ok(uuid) = id.parse::<Uuid>() {
        let _ = db::projects::delete_project(&state.pool, uuid).await;
    }
    Redirect::to("/projects")
}

// --- Log page ---

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

    let content = (LogsTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
        logs_html,
    }).render().unwrap_or_default();

    Html(render_page(&format!("Logs - {}", project_name), &pool_projects, &slug, "logs", content))
}

// --- Placeholder pages for Phase 2-4 ---

pub async fn traces_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());

    let content = (PlaceholderTemplate {
        page_title: "Trace Explorer".to_string(),
        page_icon: "lni lni-bolt".to_string(),
        page_description: "Distributed trace visualization with waterfall view, span trees, and timing analysis. Coming in Phase 2.".to_string(),
        project_slug: slug.clone(),
    }).render().unwrap_or_default();

    Html(render_page(&format!("Traces - {}", project_name), &pool_projects, &slug, "traces", content))
}

pub async fn errors_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());

    let content = (PlaceholderTemplate {
        page_title: "Error Tracking".to_string(),
        page_icon: "lni lni-warning".to_string(),
        page_description: "Errors grouped by fingerprint with occurrence counts, stack traces, and trend sparklines. Coming in Phase 3.".to_string(),
        project_slug: slug.clone(),
    }).render().unwrap_or_default();

    Html(render_page(&format!("Errors - {}", project_name), &pool_projects, &slug, "errors", content))
}

pub async fn metrics_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());

    let content = (PlaceholderTemplate {
        page_title: "Metrics Dashboard".to_string(),
        page_icon: "lni lni-bar-chart".to_string(),
        page_description: "CPU, memory, disk, network charts with process-level metrics and error correlation overlay. Coming in Phase 4.".to_string(),
        project_slug: slug.clone(),
    }).render().unwrap_or_default();

    Html(render_page(&format!("Metrics - {}", project_name), &pool_projects, &slug, "metrics", content))
}

pub async fn hosts_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());

    let content = (PlaceholderTemplate {
        page_title: "Hosts".to_string(),
        page_icon: "lni lni-server".to_string(),
        page_description: "Host overview with per-host metrics, logs, and traces. Coming in Phase 4.".to_string(),
        project_slug: slug.clone(),
    }).render().unwrap_or_default();

    Html(render_page(&format!("Hosts - {}", project_name), &pool_projects, &slug, "hosts", content))
}

// --- API endpoints ---

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
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

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
