use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Sse},
};
use axum_extra::extract::cookie::SignedCookieJar;
use askama::Template;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
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
#[template(path = "traces.html")]
struct TracesTemplate {
    project_slug: String,
    project_name: String,
    hosts: Vec<HostInfo>,
    traces_html: String,
}

#[derive(Template)]
#[template(path = "errors.html")]
struct ErrorsTemplate {
    project_slug: String,
    project_name: String,
    hosts: Vec<HostInfo>,
    errors_html: String,
}

pub struct HostDetailView {
    pub hostname: String,
    pub first_seen: String,
    pub last_seen: String,
    pub log_count: i64,
    pub span_count: i64,
}

#[derive(Template)]
#[template(path = "hosts.html")]
struct HostsTemplate {
    project_slug: String,
    project_name: String,
    hosts: Vec<HostDetailView>,
}

pub struct WaterfallSpan {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub kind_class: String,
    pub bar_class: String,
    pub status: String,
    pub status_message: String,
    pub duration_ms: f64,
    pub duration_display: String,
    pub depth: usize,
    pub indent_px: usize,
    pub offset_pct: f64,
    pub width_pct: f64,
    pub has_error: bool,
    pub attributes_json: String,
    pub timestamp: String,
}

pub struct TraceLogView {
    pub timestamp: String,
    pub level: String,
    pub level_badge: String,
    pub message: String,
    pub span_id: String,
}

#[derive(Template)]
#[template(path = "trace_detail.html")]
struct TraceDetailTemplate {
    project_slug: String,
    project_name: String,
    trace_id: String,
    trace_id_short: String,
    root_name: String,
    total_duration: String,
    span_count: usize,
    hostname: String,
    status: String,
    status_badge: String,
    timestamp: String,
    spans: Vec<WaterfallSpan>,
    logs: Vec<TraceLogView>,
    log_count: usize,
}

#[derive(Template)]
#[template(path = "metrics.html")]
struct MetricsTemplate {
    project_slug: String,
    project_name: String,
    hosts: Vec<HostInfo>,
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

// --- Traces page ---

fn render_trace_row(trace: &db::spans::TraceRow, hostname: &str, slug: &str) -> String {
    let ts = trace.timestamp.format("%Y-%m-%d %H:%M:%S");
    let duration = format_duration(trace.duration_ms);
    let status_badge = match trace.status.as_str() {
        "error" => "badge-error",
        "ok" => "badge-success",
        _ => "badge-ghost",
    };
    let escaped_name = trace.root_name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let short_tid = &trace.trace_id[..trace.trace_id.len().min(16)];

    format!(
        r#"<tr class="hover cursor-pointer" onclick="window.location.href='/projects/{}/traces/{}'">
            <td class="font-mono text-xs">{}</td>
            <td class="text-sm">{}</td>
            <td class="text-sm text-center">{}</td>
            <td class="text-sm font-mono">{}</td>
            <td><span class="badge {} badge-xs">{}</span></td>
            <td class="text-xs opacity-70">{}</td>
            <td class="text-xs opacity-70">{}</td>
        </tr>"#,
        slug, trace.trace_id,
        short_tid, escaped_name, trace.span_count, duration,
        status_badge, trace.status.to_uppercase(), hostname, ts
    )
}

pub async fn traces_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

    let hosts_raw = if let Some(pid) = project_id {
        db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default()
    } else {
        vec![]
    };
    let hosts: Vec<HostInfo> = hosts_raw.iter().map(|(id, hostname)| HostInfo {
        id: id.to_string(),
        hostname: hostname.clone(),
    }).collect();

    let traces_html = if let Some(pid) = project_id {
        let query = db::spans::SpanQuery {
            project_id: pid,
            host_id: None,
            status: None,
            search: None,
            limit: 100,
        };
        let traces = db::spans::query_traces(&state.pool, &query).await.unwrap_or_default();
        if traces.is_empty() {
            r#"<div class="text-center py-12 opacity-60">
                <i class="lni lni-bolt" style="font-size: 3rem;"></i>
                <p class="mt-4">No traces received yet. Traces will appear once your application sends OTLP trace data.</p>
            </div>"#.to_string()
        } else {
            let host_map: std::collections::HashMap<Uuid, String> = hosts_raw.iter().cloned().collect();
            let rows: String = traces.iter().map(|t| {
                let hostname = host_map.get(&t.host_id).map(|s| s.as_str()).unwrap_or("unknown");
                render_trace_row(t, hostname, &slug)
            }).collect::<Vec<_>>().join("\n");
            format!(
                r#"<div class="overflow-x-auto">
                    <table class="table table-sm">
                        <thead>
                            <tr>
                                <th>Trace ID</th>
                                <th>Root Span</th>
                                <th>Spans</th>
                                <th>Duration</th>
                                <th>Status</th>
                                <th>Host</th>
                                <th>Time</th>
                            </tr>
                        </thead>
                        <tbody>{}</tbody>
                    </table>
                </div>"#,
                rows
            )
        }
    } else {
        String::new()
    };

    let content = (TracesTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
        traces_html,
    }).render().unwrap_or_default();

    Html(render_page(&format!("Traces - {}", project_name), &pool_projects, &slug, "traces", content))
}

// --- Errors page ---

pub async fn errors_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

    let hosts_raw = if let Some(pid) = project_id {
        db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default()
    } else {
        vec![]
    };
    let hosts: Vec<HostInfo> = hosts_raw.iter().map(|(id, hostname)| HostInfo {
        id: id.to_string(),
        hostname: hostname.clone(),
    }).collect();

    let errors_html = if let Some(pid) = project_id {
        let query = db::logs::LogQuery {
            project_id: pid,
            level: Some("error".to_string()),
            host_id: None,
            search: None,
            limit: 100,
            before: None,
        };
        let logs = db::logs::query_logs(&state.pool, &query).await.unwrap_or_default();
        if logs.is_empty() {
            r#"<div class="text-center py-12 opacity-60">
                <i class="lni lni-warning" style="font-size: 3rem;"></i>
                <p class="mt-4">No errors found. That's a good thing!</p>
            </div>"#.to_string()
        } else {
            let host_map: std::collections::HashMap<Uuid, String> = hosts_raw.iter().cloned().collect();
            logs.iter().rev().map(|l| {
                let hostname = host_map.get(&l.host_id).map(|s| s.as_str()).unwrap_or("unknown");
                crate::api::logs::render_log_row(l, hostname)
            }).collect::<Vec<_>>().join("\n")
        }
    } else {
        String::new()
    };

    let content = (ErrorsTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
        errors_html,
    }).render().unwrap_or_default();

    Html(render_page(&format!("Errors - {}", project_name), &pool_projects, &slug, "errors", content))
}

// --- Metrics page ---

pub async fn metrics_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

    let hosts: Vec<HostInfo> = if let Some(pid) = project_id {
        let hosts_raw = db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default();
        hosts_raw.iter().map(|(id, name)| HostInfo {
            id: id.to_string(),
            hostname: name.clone(),
        }).collect()
    } else {
        vec![]
    };

    let content = (MetricsTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
    }).render().unwrap_or_default();

    Html(render_page(&format!("Metrics - {}", project_name), &pool_projects, &slug, "metrics", content))
}

// --- Metrics Timeseries API ---

#[derive(Deserialize)]
pub struct TimeseriesQuery {
    pub metric: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_range")]
    pub range: String,
}

fn default_range() -> String { "1h".to_string() }

#[derive(Serialize)]
pub struct TimeseriesSeries {
    pub host: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Serialize)]
pub struct TimeseriesResponse {
    pub timestamps: Vec<i64>,
    pub series: Vec<TimeseriesSeries>,
}

pub async fn metrics_timeseries_api(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<TimeseriesQuery>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_id = match pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id) {
        Some(id) => id,
        None => return Json(TimeseriesResponse { timestamps: vec![], series: vec![] }),
    };

    let since = match query.range.as_str() {
        "6h" => chrono::Utc::now() - chrono::Duration::hours(6),
        "24h" => chrono::Utc::now() - chrono::Duration::hours(24),
        "7d" => chrono::Utc::now() - chrono::Duration::days(7),
        _ => chrono::Utc::now() - chrono::Duration::hours(1),
    };

    // Resolve host filter
    let host_id = if let Some(ref h) = query.host {
        if h == "all" || h.is_empty() {
            None
        } else {
            h.parse::<Uuid>().ok()
        }
    } else {
        None
    };

    let points = db::metrics::query_metrics_timeseries(
        &state.pool, project_id, &query.metric, host_id, since,
    ).await.unwrap_or_default();

    if points.is_empty() {
        return Json(TimeseriesResponse { timestamps: vec![], series: vec![] });
    }

    // Get host names
    let hosts_raw = db::projects::list_hosts(&state.pool, project_id).await.unwrap_or_default();
    let host_map: HashMap<Uuid, String> = hosts_raw.into_iter().collect();

    // Group points by host
    let mut host_points: HashMap<String, Vec<(i64, f64)>> = HashMap::new();
    for p in &points {
        let hostname = host_map.get(&p.host_id).cloned().unwrap_or_else(|| "unknown".to_string());
        host_points.entry(hostname).or_default().push((p.timestamp.timestamp(), p.value));
    }

    // Build aligned timestamp array (union of all timestamps)
    let mut all_ts: Vec<i64> = points.iter().map(|p| p.timestamp.timestamp()).collect();
    all_ts.sort();
    all_ts.dedup();

    // Build series with aligned values
    let series: Vec<TimeseriesSeries> = host_points.into_iter().map(|(host, pts)| {
        let pts_map: HashMap<i64, f64> = pts.into_iter().collect();
        let values: Vec<Option<f64>> = all_ts.iter().map(|ts| pts_map.get(ts).copied()).collect();
        TimeseriesSeries { host, values }
    }).collect();

    Json(TimeseriesResponse { timestamps: all_ts, series })
}

// --- SSE Metrics ---

pub async fn sse_metrics(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let mut rx = state.sse.subscribe_metrics(&slug).await;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    yield Ok(axum::response::sse::Event::default()
                        .event("metrics")
                        .data(event.json));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE metrics client lagged by {} messages", n);
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

// --- Hosts page ---

pub async fn hosts_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

    let hosts: Vec<HostDetailView> = if let Some(pid) = project_id {
        let details = db::projects::list_hosts_detailed(&state.pool, pid).await.unwrap_or_default();
        details.iter().map(|h| HostDetailView {
            hostname: h.hostname.clone(),
            first_seen: h.first_seen.format("%Y-%m-%d %H:%M").to_string(),
            last_seen: h.last_seen.format("%Y-%m-%d %H:%M").to_string(),
            log_count: h.log_count,
            span_count: h.span_count,
        }).collect()
    } else {
        vec![]
    };

    let content = (HostsTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        hosts,
    }).render().unwrap_or_default();

    Html(render_page(&format!("Hosts - {}", project_name), &pool_projects, &slug, "hosts", content))
}

// --- Trace detail page ---

fn format_duration(ms: f64) -> String {
    if ms < 0.001 {
        format!("{:.0}ns", ms * 1_000_000.0)
    } else if ms < 1.0 {
        format!("{:.0}us", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.1}ms", ms)
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

fn build_waterfall(spans: &[db::spans::SpanEntry]) -> Vec<WaterfallSpan> {
    if spans.is_empty() {
        return vec![];
    }

    // Find trace time bounds
    let trace_start = spans.iter().map(|s| s.timestamp).min().unwrap();
    let trace_end = spans.iter().map(|s| {
        s.timestamp + chrono::Duration::microseconds((s.duration_ms * 1000.0) as i64)
    }).max().unwrap();
    let total_duration_ms = (trace_end - trace_start).num_microseconds().unwrap_or(1) as f64 / 1000.0;
    let total_duration_ms = if total_duration_ms < 0.001 { 0.001 } else { total_duration_ms };

    // Build parent→children index
    let mut id_to_idx: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, span) in spans.iter().enumerate() {
        id_to_idx.insert(&span.id, i);
    }

    let mut children: Vec<Vec<usize>> = vec![vec![]; spans.len()];
    let mut roots: Vec<usize> = vec![];

    for (i, span) in spans.iter().enumerate() {
        if let Some(ref parent_id) = span.parent_span_id {
            if let Some(&parent_idx) = id_to_idx.get(parent_id.as_str()) {
                children[parent_idx].push(i);
            } else {
                roots.push(i);
            }
        } else {
            roots.push(i);
        }
    }

    // Sort roots and children by timestamp
    roots.sort_by_key(|&i| spans[i].timestamp);
    for ch in &mut children {
        ch.sort_by_key(|&i| spans[i].timestamp);
    }

    // DFS to build ordered list with depth
    let mut result = Vec::new();
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&i| (i, 0)).collect();

    while let Some((idx, depth)) = stack.pop() {
        let span = &spans[idx];
        let offset_ms = (span.timestamp - trace_start).num_microseconds().unwrap_or(0) as f64 / 1000.0;
        let offset_pct = (offset_ms / total_duration_ms * 100.0).clamp(0.0, 100.0);
        let width_pct = (span.duration_ms / total_duration_ms * 100.0).clamp(0.5, 100.0 - offset_pct);

        let attrs_formatted = if let Some(obj) = span.attributes.as_object() {
            serde_json::to_string_pretty(obj).unwrap_or_else(|_| "{}".to_string())
        } else {
            "{}".to_string()
        };

        let (kind_class, bar_class) = match span.kind.as_str() {
            "server" => ("bg-blue-500/20 text-blue-300", "bg-blue-500"),
            "client" => ("bg-green-500/20 text-green-300", "bg-green-500"),
            "internal" => ("bg-gray-500/20 text-gray-300", "bg-gray-400"),
            "producer" => ("bg-purple-500/20 text-purple-300", "bg-purple-500"),
            "consumer" => ("bg-orange-500/20 text-orange-300", "bg-orange-500"),
            _ => ("bg-gray-500/20 text-gray-400", "bg-primary"),
        };
        let (kind_class, bar_class) = if span.status == "error" {
            (kind_class, "bg-error")
        } else {
            (kind_class, bar_class)
        };

        result.push(WaterfallSpan {
            id: span.id.clone(),
            name: span.name.clone(),
            kind: span.kind.clone(),
            kind_class: kind_class.to_string(),
            bar_class: bar_class.to_string(),
            status: span.status.clone(),
            status_message: span.status_message.clone().unwrap_or_default(),
            duration_ms: span.duration_ms,
            duration_display: format_duration(span.duration_ms),
            depth,
            indent_px: depth * 20,
            offset_pct,
            width_pct,
            has_error: span.status == "error",
            attributes_json: attrs_formatted,
            timestamp: span.timestamp.format("%H:%M:%S%.3f").to_string(),
        });

        // Push children in reverse order so first child is processed first
        for &child_idx in children[idx].iter().rev() {
            stack.push((child_idx, depth + 1));
        }
    }

    result
}

pub async fn trace_detail_page(
    State(state): State<AppState>,
    Path((slug, trace_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_name = pool_projects.iter().find(|(_, _, s)| s == &slug)
        .map(|(_, n, _)| n.clone()).unwrap_or_else(|| slug.clone());
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug)
        .map(|(id, _, _)| *id);

    let spans = db::spans::query_spans_by_trace(&state.pool, &trace_id).await.unwrap_or_default();
    let related_logs = db::logs::query_logs_by_trace(&state.pool, &trace_id).await.unwrap_or_default();

    let hosts_raw = if let Some(pid) = project_id {
        db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default()
    } else {
        vec![]
    };
    let host_map: std::collections::HashMap<Uuid, String> = hosts_raw.into_iter().collect();

    // Build waterfall
    let waterfall = build_waterfall(&spans);

    // Trace-level summary
    let root_name = spans.first().map(|s| s.name.clone()).unwrap_or_else(|| "Unknown".to_string());
    let total_duration_ms: f64 = spans.iter().map(|s| s.duration_ms).fold(0.0_f64, f64::max);
    let hostname = spans.first()
        .map(|s| host_map.get(&s.host_id).cloned().unwrap_or_else(|| "unknown".to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let status = spans.iter().find(|s| s.status == "error")
        .map(|_| "error".to_string())
        .unwrap_or_else(|| spans.first().map(|s| s.status.clone()).unwrap_or_else(|| "unset".to_string()));
    let status_badge = match status.as_str() {
        "error" => "badge-error",
        "ok" => "badge-success",
        _ => "badge-ghost",
    };
    let timestamp = spans.first()
        .map(|s| s.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_default();

    let trace_id_short = trace_id[..trace_id.len().min(16)].to_string();

    // Related logs
    let logs: Vec<TraceLogView> = related_logs.iter().map(|l| {
        let level_badge = match l.level.as_str() {
            "error" | "ERROR" => "badge-error",
            "warn" | "WARN" => "badge-warning",
            "info" | "INFO" => "badge-info",
            "debug" | "DEBUG" => "badge-ghost",
            _ => "badge-ghost",
        };
        TraceLogView {
            timestamp: l.timestamp.format("%H:%M:%S%.3f").to_string(),
            level: l.level.to_uppercase(),
            level_badge: level_badge.to_string(),
            message: l.message.clone(),
            span_id: l.span_id.clone().unwrap_or_default(),
        }
    }).collect();

    let content = (TraceDetailTemplate {
        project_slug: slug.clone(),
        project_name: project_name.clone(),
        trace_id: trace_id.clone(),
        trace_id_short,
        root_name,
        total_duration: format_duration(total_duration_ms),
        span_count: spans.len(),
        hostname,
        status: status.to_uppercase(),
        status_badge: status_badge.to_string(),
        timestamp,
        log_count: logs.len(),
        spans: waterfall,
        logs,
    }).render().unwrap_or_default();

    Html(render_page(
        &format!("Trace {} - {}", &trace_id[..trace_id.len().min(8)], project_name),
        &pool_projects,
        &slug,
        "traces",
        content,
    ))
}

// --- API endpoints ---

fn empty_string_as_none<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    let s: Option<String> = Option::deserialize(de)?;
    Ok(s.filter(|v| !v.is_empty()))
}

fn empty_string_as_none_uuid<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Uuid>, D::Error> {
    let s: Option<String> = Option::deserialize(de)?;
    Ok(s.filter(|v| !v.is_empty()).and_then(|v| v.parse::<Uuid>().ok()))
}

#[derive(Deserialize)]
pub struct LogsQuery {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    level: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none_uuid")]
    host_id: Option<Uuid>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
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

#[derive(Deserialize)]
pub struct TracesQuery {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    status: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none_uuid")]
    host_id: Option<Uuid>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    search: Option<String>,
}

pub async fn traces_data(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<TracesQuery>,
) -> impl IntoResponse {
    let pool_projects = db::projects::list_projects(&state.pool).await.unwrap_or_default();
    let project_id = pool_projects.iter().find(|(_, _, s)| s == &slug).map(|(id, _, _)| *id);

    let Some(pid) = project_id else {
        return Html(String::new());
    };

    let hosts = db::projects::list_hosts(&state.pool, pid).await.unwrap_or_default();
    let host_map: std::collections::HashMap<Uuid, String> = hosts.into_iter().collect();

    let query = db::spans::SpanQuery {
        project_id: pid,
        host_id: params.host_id,
        status: params.status,
        search: params.search,
        limit: 100,
    };

    let traces = db::spans::query_traces(&state.pool, &query).await.unwrap_or_default();
    if traces.is_empty() {
        return Html(r#"<div class="text-center py-12 opacity-60">
            <p>No matching traces found.</p>
        </div>"#.to_string());
    }

    let rows: String = traces.iter().map(|t| {
        let hostname = host_map.get(&t.host_id).map(|s| s.as_str()).unwrap_or("unknown");
        render_trace_row(t, hostname, &slug)
    }).collect::<Vec<_>>().join("\n");

    Html(format!(
        r#"<div class="overflow-x-auto">
            <table class="table table-sm">
                <thead>
                    <tr>
                        <th>Trace ID</th>
                        <th>Root Span</th>
                        <th>Spans</th>
                        <th>Duration</th>
                        <th>Status</th>
                        <th>Host</th>
                        <th>Time</th>
                    </tr>
                </thead>
                <tbody>{}</tbody>
            </table>
        </div>"#,
        rows
    ))
}

#[derive(Deserialize)]
pub struct ErrorsQuery {
    #[serde(default, deserialize_with = "empty_string_as_none_uuid")]
    host_id: Option<Uuid>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    search: Option<String>,
}

pub async fn errors_data(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<ErrorsQuery>,
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
        level: Some("error".to_string()),
        host_id: params.host_id,
        search: params.search,
        limit: 100,
        before: None,
    };

    let logs = db::logs::query_logs(&state.pool, &query).await.unwrap_or_default();
    if logs.is_empty() {
        return Html(r#"<div class="text-center py-12 opacity-60">
            <p>No matching errors found.</p>
        </div>"#.to_string());
    }

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
