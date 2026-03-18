use axum::{extract::State, Json};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::otlp_types::{attributes_to_json, nanos_to_datetime, InstrumentationScope, KeyValue, Resource};
use crate::db;
use crate::db::metrics::{batch_insert_metrics, MetricEntry};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportMetricsServiceRequest {
    #[serde(rename = "resourceMetrics", default)]
    pub resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceMetrics {
    #[serde(default)]
    pub resource: Option<Resource>,
    #[serde(rename = "scopeMetrics", default)]
    pub scope_metrics: Vec<ScopeMetrics>,
}

#[derive(Debug, Deserialize)]
pub struct ScopeMetrics {
    #[serde(default)]
    pub scope: Option<InstrumentationScope>,
    #[serde(default)]
    pub metrics: Vec<Metric>,
}

#[derive(Debug, Deserialize)]
pub struct Metric {
    pub name: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub gauge: Option<Gauge>,
    #[serde(default)]
    pub sum: Option<Sum>,
}

#[derive(Debug, Deserialize)]
pub struct Gauge {
    #[serde(rename = "dataPoints", default)]
    pub data_points: Vec<NumberDataPoint>,
}

#[derive(Debug, Deserialize)]
pub struct Sum {
    #[serde(rename = "dataPoints", default)]
    pub data_points: Vec<NumberDataPoint>,
}

#[derive(Debug, Deserialize)]
pub struct NumberDataPoint {
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
    #[serde(rename = "timeUnixNano")]
    pub time_unix_nano: String,
    #[serde(rename = "asInt", default)]
    pub as_int: Option<String>,
    #[serde(rename = "asDouble", default)]
    pub as_double: Option<f64>,
}

pub async fn ingest_metrics(
    State(state): State<AppState>,
    Json(req): Json<ExportMetricsServiceRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut all_metrics = Vec::new();

    for resource_metric in req.resource_metrics {
        // Extract resource attributes
        let service_name = resource_metric
            .resource
            .as_ref()
            .map(|r| r.service_name())
            .unwrap_or_else(|| "unknown".to_string());

        let hostname = resource_metric
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

        // Process metrics
        for scope_metric in resource_metric.scope_metrics {
            for metric in scope_metric.metrics {
                let unit = metric.unit.unwrap_or_else(|| "".to_string());

                // Collect data points from gauge or sum
                let data_points = if let Some(gauge) = metric.gauge {
                    gauge.data_points
                } else if let Some(sum) = metric.sum {
                    sum.data_points
                } else {
                    Vec::new()
                };

                // Convert each data point to MetricEntry
                for point in data_points {
                    let timestamp = nanos_to_datetime(&point.time_unix_nano);

                    // Extract value (prefer asDouble, fallback to asInt)
                    let value = if let Some(double_val) = point.as_double {
                        double_val
                    } else if let Some(int_str) = point.as_int {
                        int_str.parse::<i64>().unwrap_or(0) as f64
                    } else {
                        0.0
                    };

                    let entry = MetricEntry {
                        id: Uuid::new_v4(),
                        project_id,
                        host_id,
                        timestamp,
                        metric_name: metric.name.clone(),
                        value,
                        unit: unit.clone(),
                        attributes: attributes_to_json(&point.attributes),
                    };

                    all_metrics.push(entry);
                }
            }
        }
    }

    let metric_count = all_metrics.len();

    batch_insert_metrics(&state.pool, &all_metrics)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!("Ingested {} metrics via OTLP", metric_count);

    Ok(StatusCode::OK)
}
