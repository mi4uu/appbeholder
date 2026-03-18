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

pub struct TraceRow {
    pub trace_id: String,
    pub root_name: String,
    pub span_count: i64,
    pub duration_ms: f64,
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub host_id: Uuid,
}

pub struct SpanQuery {
    pub project_id: Uuid,
    pub host_id: Option<Uuid>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub limit: i64,
}

pub async fn query_traces(pool: &Pool, query: &SpanQuery) -> Result<Vec<TraceRow>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    let mut sql = String::from(
        "SELECT trace_id, MIN(name) AS root_name, COUNT(*)::bigint AS span_count,
                MAX(duration_ms) AS duration_ms, MAX(status) AS status,
                MIN(timestamp) AS timestamp, MIN(host_id) AS host_id
         FROM spans WHERE project_id = $1"
    );
    let mut param_idx = 2u32;
    let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![Box::new(query.project_id)];

    if let Some(ref host_id) = query.host_id {
        sql.push_str(&format!(" AND host_id = ${}", param_idx));
        params.push(Box::new(*host_id));
        param_idx += 1;
    }

    if let Some(ref status) = query.status {
        sql.push_str(&format!(" AND status = ${}", param_idx));
        params.push(Box::new(status.clone()));
        param_idx += 1;
    }

    if let Some(ref search) = query.search {
        sql.push_str(&format!(" AND name ILIKE ${}", param_idx));
        params.push(Box::new(format!("%{}%", search)));
        param_idx += 1;
    }

    let _ = param_idx;

    sql.push_str(" GROUP BY trace_id ORDER BY MIN(timestamp) DESC LIMIT $");
    sql.push_str(&params.len().wrapping_add(1).to_string());
    params.push(Box::new(query.limit));

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    let rows = client.query(&sql, &param_refs).await?;

    Ok(rows.iter().map(|r| TraceRow {
        trace_id: r.get(0),
        root_name: r.get(1),
        span_count: r.get(2),
        duration_ms: r.get(3),
        status: r.get(4),
        timestamp: r.get(5),
        host_id: r.get(6),
    }).collect())
}

pub async fn query_spans_by_trace(pool: &Pool, trace_id: &str) -> Result<Vec<SpanEntry>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT id, trace_id, parent_span_id, project_id, host_id, timestamp, duration_ms, name, kind, status, status_message, attributes
         FROM spans WHERE trace_id = $1 ORDER BY timestamp ASC",
        &[&trace_id],
    ).await?;

    Ok(rows.iter().map(|r| SpanEntry {
        id: r.get(0),
        trace_id: r.get(1),
        parent_span_id: r.get(2),
        project_id: r.get(3),
        host_id: r.get(4),
        timestamp: r.get(5),
        duration_ms: r.get(6),
        name: r.get(7),
        kind: r.get(8),
        status: r.get(9),
        status_message: r.get(10),
        attributes: r.get(11),
    }).collect())
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
