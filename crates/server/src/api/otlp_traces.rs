use axum::{extract::State, Json};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::api::otlp_types::{attributes_to_json, nanos_to_datetime, InstrumentationScope, KeyValue, Resource};
use crate::db;
use crate::db::spans::{batch_insert_spans, SpanEntry};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportTraceServiceRequest {
    #[serde(rename = "resourceSpans", default)]
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceSpans {
    #[serde(default)]
    pub resource: Option<Resource>,
    #[serde(rename = "scopeSpans", default)]
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Debug, Deserialize)]
pub struct ScopeSpans {
    #[serde(default)]
    pub scope: Option<InstrumentationScope>,
    #[serde(default)]
    pub spans: Vec<Span>,
}

#[derive(Debug, Deserialize)]
pub struct Span {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "parentSpanId", default)]
    pub parent_span_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub kind: i32,
    #[serde(rename = "startTimeUnixNano")]
    pub start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    pub end_time_unix_nano: String,
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
    #[serde(default)]
    pub status: Option<SpanStatus>,
}

#[derive(Debug, Deserialize)]
pub struct SpanStatus {
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub message: Option<String>,
}

fn span_kind_to_string(kind: i32) -> String {
    match kind {
        1 => "internal".to_string(),
        2 => "server".to_string(),
        3 => "client".to_string(),
        4 => "producer".to_string(),
        5 => "consumer".to_string(),
        _ => "unspecified".to_string(),
    }
}

fn status_code_to_string(code: i32) -> String {
    match code {
        1 => "ok".to_string(),
        2 => "error".to_string(),
        _ => "unset".to_string(),
    }
}

fn is_valid_span_id(id: &Option<String>) -> bool {
    if let Some(s) = id {
        !s.is_empty() && !s.chars().all(|c| c == '0')
    } else {
        false
    }
}

pub async fn ingest_traces(
    State(state): State<AppState>,
    Json(req): Json<ExportTraceServiceRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut all_spans = Vec::new();

    for resource_span in req.resource_spans {
        // Extract resource attributes
        let service_name = resource_span
            .resource
            .as_ref()
            .map(|r| r.service_name())
            .unwrap_or_else(|| "unknown".to_string());

        let hostname = resource_span
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

        // Process spans
        for scope_span in resource_span.scope_spans {
            for span in scope_span.spans {
                let start_nanos = span.start_time_unix_nano.parse::<u64>().unwrap_or(0);
                let end_nanos = span.end_time_unix_nano.parse::<u64>().unwrap_or(0);
                let duration_ms = if end_nanos > start_nanos {
                    (end_nanos - start_nanos) as f64 / 1_000_000.0
                } else {
                    0.0
                };

                let timestamp = nanos_to_datetime(&span.start_time_unix_nano);
                let kind = span_kind_to_string(span.kind);
                let status = span
                    .status
                    .as_ref()
                    .map(|s| status_code_to_string(s.code))
                    .unwrap_or_else(|| "unset".to_string());

                let status_message = span.status.as_ref().and_then(|s| s.message.clone());

                // Filter parent_span_id — skip if empty or all zeros
                let parent_span_id = if is_valid_span_id(&span.parent_span_id) {
                    span.parent_span_id
                } else {
                    None
                };

                let entry = SpanEntry {
                    id: span.span_id.to_lowercase(),
                    trace_id: span.trace_id.to_lowercase(),
                    parent_span_id: parent_span_id.map(|s| s.to_lowercase()),
                    project_id,
                    host_id,
                    timestamp,
                    duration_ms,
                    name: span.name,
                    kind,
                    status,
                    status_message,
                    attributes: attributes_to_json(&span.attributes),
                };

                all_spans.push(entry);
            }
        }
    }

    let span_count = all_spans.len();

    batch_insert_spans(&state.pool, &all_spans)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!("Ingested {} spans via OTLP", span_count);

    Ok(StatusCode::OK)
}
