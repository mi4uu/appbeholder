use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::body::Bytes;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::logs::render_log_row;
use crate::api::otlp_types::{attributes_to_json, nanos_to_datetime, AnyValue, InstrumentationScope, KeyValue, Resource};
use crate::db;
use crate::db::logs::{insert_log, LogEntry};
use crate::sse::channels::LogEvent;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportLogsServiceRequest {
    #[serde(rename = "resourceLogs", default)]
    pub resource_logs: Vec<ResourceLogs>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceLogs {
    #[serde(default)]
    pub resource: Option<Resource>,
    #[serde(rename = "scopeLogs", default)]
    pub scope_logs: Vec<ScopeLogs>,
}

#[derive(Debug, Deserialize)]
pub struct ScopeLogs {
    #[serde(default)]
    pub scope: Option<InstrumentationScope>,
    #[serde(rename = "logRecords", default)]
    pub log_records: Vec<LogRecord>,
}

#[derive(Debug, Deserialize)]
pub struct LogRecord {
    #[serde(rename = "timeUnixNano")]
    pub time_unix_nano: String,
    #[serde(rename = "severityNumber", default)]
    pub severity_number: i32,
    #[serde(rename = "severityText", default)]
    pub severity_text: Option<String>,
    #[serde(default)]
    pub body: Option<AnyValue>,
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
    #[serde(rename = "traceId", default)]
    pub trace_id: Option<String>,
    #[serde(rename = "spanId", default)]
    pub span_id: Option<String>,
}

fn severity_to_level(severity_number: i32, severity_text: Option<String>) -> String {
    // Prefer explicit text if provided and non-empty
    if let Some(ref text) = severity_text {
        if !text.is_empty() {
            return text.to_lowercase();
        }
    }

    // Map OTLP severity number to level
    match severity_number {
        1..=8 => "debug".to_string(),
        9..=12 => "info".to_string(),
        13..=16 => "warn".to_string(),
        17..=20 => "error".to_string(),
        21..=24 => "fatal".to_string(),
        _ => "info".to_string(),
    }
}

fn is_valid_id(id: &Option<String>) -> bool {
    if let Some(s) = id {
        !s.is_empty() && !s.chars().all(|c| c == '0')
    } else {
        false
    }
}

pub async fn ingest_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    let content_type = headers.get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let req: ExportLogsServiceRequest = serde_json::from_slice(&body)
        .map_err(|e| {
            let preview = String::from_utf8_lossy(&body[..body.len().min(200)]);
            tracing::error!(
                content_type = content_type,
                body_len = body.len(),
                body_preview = %preview,
                error = %e,
                "Failed to parse OTLP logs request"
            );
            (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}. Content-Type: {}", e, content_type))
        })?;
    let mut log_count = 0;

    for resource_log in req.resource_logs {
        // Extract resource attributes
        let service_name = resource_log
            .resource
            .as_ref()
            .map(|r| r.service_name())
            .unwrap_or_else(|| "unknown".to_string());

        let hostname = resource_log
            .resource
            .as_ref()
            .map(|r| r.host_name())
            .unwrap_or_else(|| "unknown".to_string());

        // Get or create project and host
        let project_id = db::projects::get_or_create_project(&state.pool, &service_name)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let host_id = db::projects::get_or_create_host(&state.pool, project_id, &hostname)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Process log records
        for scope_log in resource_log.scope_logs {
            for record in scope_log.log_records {
                let timestamp = nanos_to_datetime(&record.time_unix_nano);
                let level = severity_to_level(record.severity_number, record.severity_text);
                let message = record
                    .body
                    .as_ref()
                    .and_then(|b| b.as_string())
                    .unwrap_or_else(|| "".to_string());

                // Filter trace_id and span_id — skip if empty or all zeros
                let trace_id = if is_valid_id(&record.trace_id) {
                    record.trace_id.map(|s| s.to_lowercase())
                } else {
                    None
                };

                let span_id = if is_valid_id(&record.span_id) {
                    record.span_id.map(|s| s.to_lowercase())
                } else {
                    None
                };

                let entry = LogEntry {
                    id: Uuid::new_v4(),
                    project_id,
                    host_id,
                    timestamp,
                    level,
                    message,
                    source: "backend".to_string(),
                    trace_id,
                    span_id,
                    fingerprint: None,
                    attributes: attributes_to_json(&record.attributes),
                    stack_trace: None,
                };

                insert_log(&state.pool, &entry)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                // Publish to SSE
                let html = render_log_row(&entry, &hostname);
                state.sse.publish_log(&service_name, LogEvent { html }).await;

                log_count += 1;
            }
        }
    }

    tracing::info!("Ingested {} logs via OTLP", log_count);

    Ok(StatusCode::OK)
}
