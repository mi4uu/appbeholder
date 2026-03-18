use axum::{extract::State, http::HeaderMap, Json};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Sha256, Digest};
use uuid::Uuid;

use crate::db;
use crate::db::logs::{insert_log, LogEntry};
use crate::sse::channels::LogEvent;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ErrorRequest {
    pub message: String,
    pub stack_trace: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: Option<JsonValue>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub id: Uuid,
    pub status: String,
}

fn compute_fingerprint(message: &str, stack_trace: &Option<String>) -> String {
    let normalized = normalize_message(message);
    let first_frame = stack_trace
        .as_ref()
        .and_then(|st| st.lines().nth(1)) // Skip first line (error message), take first frame
        .unwrap_or("");

    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hasher.update(first_frame.as_bytes());
    hex::encode(hasher.finalize())
}

fn normalize_message(msg: &str) -> String {
    let mut result = String::with_capacity(msg.len());
    let mut in_number = false;

    for c in msg.chars() {
        if c.is_ascii_digit() {
            if !in_number {
                result.push('#');
                in_number = true;
            }
        } else {
            in_number = false;
            result.push(c);
        }
    }

    result
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

pub async fn ingest_error(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ErrorRequest>,
) -> Result<Json<ErrorResponse>, (StatusCode, String)> {
    let slug = extract_project_slug(&headers)?;
    let hostname = extract_hostname(&headers);

    let project_id = db::projects::get_or_create_project(&state.pool, &slug)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let host_id = db::projects::get_or_create_host(&state.pool, project_id, &hostname)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let fingerprint = compute_fingerprint(&req.message, &req.stack_trace);

    let entry = LogEntry {
        id: Uuid::new_v4(),
        project_id,
        host_id,
        timestamp: req.timestamp.unwrap_or_else(Utc::now),
        level: "error".to_string(),
        message: req.message,
        source: req.source.unwrap_or_else(|| "backend".to_string()),
        trace_id: req.trace_id,
        span_id: req.span_id,
        fingerprint: Some(fingerprint),
        attributes: req.attributes.unwrap_or(serde_json::json!({})),
        stack_trace: req.stack_trace,
    };

    insert_log(&state.pool, &entry)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Publish to SSE (reuse log rendering from api::logs)
    let html = crate::api::logs::render_log_row(&entry, &hostname);
    state.sse.publish_log(&slug, LogEvent { html }).await;

    Ok(Json(ErrorResponse {
        id: entry.id,
        status: "ok".to_string(),
    }))
}
