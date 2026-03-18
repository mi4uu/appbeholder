use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use uuid::Uuid;

pub struct SpanEntry {
    pub id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub project_id: Uuid,
    pub host_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: f64,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub status_message: Option<String>,
    pub attributes: JsonValue,
}

pub async fn insert_span(pool: &Pool, entry: &SpanEntry) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    client.execute(
        "INSERT INTO spans (id, trace_id, parent_span_id, project_id, host_id, timestamp, duration_ms, name, kind, status, status_message, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        &[
            &entry.id, &entry.trace_id, &entry.parent_span_id,
            &entry.project_id, &entry.host_id, &entry.timestamp,
            &entry.duration_ms, &entry.name, &entry.kind,
            &entry.status, &entry.status_message, &entry.attributes,
        ],
    ).await?;
    Ok(())
}

pub async fn batch_insert_spans(pool: &Pool, entries: &[SpanEntry]) -> Result<(), Box<dyn std::error::Error>> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut client = pool.get().await?;
    let transaction = client.transaction().await?;

    let stmt = transaction.prepare(
        "INSERT INTO spans (id, trace_id, parent_span_id, project_id, host_id, timestamp, duration_ms, name, kind, status, status_message, attributes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
    ).await?;

    for entry in entries {
        transaction.execute(
            &stmt,
            &[
                &entry.id, &entry.trace_id, &entry.parent_span_id,
                &entry.project_id, &entry.host_id, &entry.timestamp,
                &entry.duration_ms, &entry.name, &entry.kind,
                &entry.status, &entry.status_message, &entry.attributes,
            ],
        ).await?;
    }

    transaction.commit().await?;
    Ok(())
}
