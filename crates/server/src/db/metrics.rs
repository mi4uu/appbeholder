use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use uuid::Uuid;

pub struct MetricEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub host_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub metric_name: String,
    pub value: f64,
    pub unit: String,
    pub attributes: JsonValue,
}

pub async fn insert_metric(pool: &Pool, entry: &MetricEntry) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    client.execute(
        "INSERT INTO metrics (id, project_id, host_id, timestamp, metric_name, value, unit, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[
            &entry.id, &entry.project_id, &entry.host_id, &entry.timestamp,
            &entry.metric_name, &entry.value, &entry.unit, &entry.attributes,
        ],
    ).await?;
    Ok(())
}

pub struct MetricSummary {
    pub metric_name: String,
    pub value: f64,
    pub unit: String,
    pub host_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

pub async fn query_metrics_summary(pool: &Pool, project_id: Uuid) -> Result<Vec<MetricSummary>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT DISTINCT ON (metric_name, host_id) metric_name, value, unit, host_id, timestamp
         FROM metrics WHERE project_id = $1
         ORDER BY metric_name, host_id, timestamp DESC
         LIMIT 200",
        &[&project_id],
    ).await?;

    Ok(rows.iter().map(|r| MetricSummary {
        metric_name: r.get(0),
        value: r.get(1),
        unit: r.get(2),
        host_id: r.get(3),
        timestamp: r.get(4),
    }).collect())
}

pub async fn batch_insert_metrics(pool: &Pool, entries: &[MetricEntry]) -> Result<(), Box<dyn std::error::Error>> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut client = pool.get().await?;
    let transaction = client.transaction().await?;

    let stmt = transaction.prepare(
        "INSERT INTO metrics (id, project_id, host_id, timestamp, metric_name, value, unit, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    ).await?;

    for entry in entries {
        transaction.execute(
            &stmt,
            &[
                &entry.id, &entry.project_id, &entry.host_id, &entry.timestamp,
                &entry.metric_name, &entry.value, &entry.unit, &entry.attributes,
            ],
        ).await?;
    }

    transaction.commit().await?;
    Ok(())
}

pub struct TimeseriesPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub host_id: Uuid,
}

pub async fn query_metrics_timeseries(
    pool: &Pool,
    project_id: Uuid,
    metric_name: &str,
    host_id: Option<Uuid>,
    since: DateTime<Utc>,
) -> Result<Vec<TimeseriesPoint>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    let rows = if let Some(hid) = host_id {
        client.query(
            "SELECT timestamp, value, host_id FROM metrics
             WHERE project_id = $1 AND metric_name = $2 AND host_id = $3 AND timestamp >= $4
             ORDER BY timestamp ASC",
            &[&project_id, &metric_name, &hid, &since],
        ).await?
    } else {
        client.query(
            "SELECT timestamp, value, host_id FROM metrics
             WHERE project_id = $1 AND metric_name = $2 AND timestamp >= $3
             ORDER BY timestamp ASC",
            &[&project_id, &metric_name, &since],
        ).await?
    };

    Ok(rows.iter().map(|r| TimeseriesPoint {
        timestamp: r.get(0),
        value: r.get(1),
        host_id: r.get(2),
    }).collect())
}
