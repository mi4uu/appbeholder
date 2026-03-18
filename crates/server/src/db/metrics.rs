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
