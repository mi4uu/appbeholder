use axum::{extract::State, http::HeaderMap, Json};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::db;
use crate::db::logs::{insert_log, LogEntry};
use crate::sse::channels::LogEvent;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct LogRequest {
    pub level: String,
    pub message: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Option<JsonValue>,
    pub stack_trace: Option<String>,
}

#[derive(Serialize)]
pub struct LogResponse {
    pub id: Uuid,
    pub status: String,
}

fn extract_project_slug(headers: &HeaderMap) -> Result<String, (StatusCode, String)> {
    headers
        .get("X-Project-Slug")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            (StatusCode::BAD_REQUEST, "Missing X-Project-Slug header".to_string())
        })
}

fn extract_hostname(headers: &HeaderMap) -> String {
    headers
        .get("X-Host")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn level_badge_class(level: &str) -> &'static str {
    match level {
        "debug" => "badge-ghost",
        "info" => "badge-info",
        "warn" => "badge-warning",
        "error" => "badge-error",
        "fatal" => "badge-error",
        _ => "badge-ghost",
    }
}

pub fn render_log_row(entry: &LogEntry, hostname: &str) -> String {
    let ts = entry.timestamp.format("%H:%M:%S%.3f");
    let badge = level_badge_class(&entry.level);
    let level_upper = entry.level.to_uppercase();
    let escaped_msg = entry.message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let has_details = entry.stack_trace.is_some() || entry.attributes != serde_json::json!({});
    let expand_attrs = if has_details {
        let mut details = String::new();
        if let Some(ref st) = entry.stack_trace {
            let escaped_st = st.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            details.push_str(&format!(
                r#"<pre class="mt-2 text-xs bg-base-300 p-2 rounded overflow-x-auto">{}</pre>"#,
                escaped_st
            ));
        }
        if entry.attributes != serde_json::json!({}) {
            let attrs_str = serde_json::to_string_pretty(&entry.attributes).unwrap_or_default();
            let escaped_attrs = attrs_str.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            details.push_str(&format!(
                r#"<pre class="mt-2 text-xs bg-base-300 p-2 rounded overflow-x-auto">{}</pre>"#,
                escaped_attrs
            ));
        }
        format!(
            r#"<div class="collapse collapse-arrow bg-base-200 mt-1">
                <input type="checkbox" />
                <div class="collapse-title text-xs p-1 min-h-0">Details</div>
                <div class="collapse-content p-2">{}</div>
            </div>"#,
            details
        )
    } else {
        String::new()
    };

    let trace_link = entry.trace_id.as_ref().map(|tid| {
        format!(r#" <span class="badge badge-ghost badge-xs">trace:{}</span>"#, &tid[..tid.len().min(8)])
    }).unwrap_or_default();

    format!(
        r#"<div class="flex items-start gap-2 py-1 px-2 border-b border-base-200 hover:bg-base-200 text-sm font-mono" id="log-{}">
            <span class="text-xs opacity-60 shrink-0">{}</span>
            <span class="badge {} badge-xs shrink-0">{}</span>
            <span class="text-xs opacity-60 shrink-0">{}</span>
            <span class="flex-1">{}{}</span>
            {}
        </div>"#,
        entry.id, ts, badge, level_upper, hostname, escaped_msg, trace_link, expand_attrs
    )
}

pub async fn ingest_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LogRequest>,
) -> Result<Json<LogResponse>, (StatusCode, String)> {
    let slug = extract_project_slug(&headers)?;
    let hostname = extract_hostname(&headers);

    let project_id = db::projects::get_or_create_project(&state.pool, &slug)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let host_id = db::projects::get_or_create_host(&state.pool, project_id, &hostname)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entry = LogEntry {
        id: Uuid::new_v4(),
        project_id,
        host_id,
        timestamp: req.timestamp.unwrap_or_else(Utc::now),
        level: req.level,
        message: req.message,
        source: req.source.unwrap_or_else(|| "backend".to_string()),
        trace_id: req.trace_id,
        span_id: req.span_id,
        fingerprint: None,
        attributes: req.attributes.unwrap_or(serde_json::json!({})),
        stack_trace: req.stack_trace,
    };

    insert_log(&state.pool, &entry)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Publish to SSE
    let html = render_log_row(&entry, &hostname);
    state.sse.publish_log(&slug, LogEvent { html }).await;

    Ok(Json(LogResponse {
        id: entry.id,
        status: "ok".to_string(),
    }))
}
