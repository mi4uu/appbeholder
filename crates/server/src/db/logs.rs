use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use uuid::Uuid;

pub struct LogEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub host_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub message: String,
    pub source: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub fingerprint: Option<String>,
    pub attributes: JsonValue,
    pub stack_trace: Option<String>,
}

pub async fn insert_log(pool: &Pool, entry: &LogEntry) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    client.execute(
        "INSERT INTO log_entries (id, project_id, host_id, timestamp, level, message, source, trace_id, span_id, fingerprint, attributes, stack_trace)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        &[
            &entry.id, &entry.project_id, &entry.host_id, &entry.timestamp,
            &entry.level, &entry.message, &entry.source,
            &entry.trace_id, &entry.span_id, &entry.fingerprint,
            &entry.attributes, &entry.stack_trace,
        ],
    ).await?;
    Ok(())
}

pub struct LogQuery {
    pub project_id: Uuid,
    pub level: Option<String>,
    pub host_id: Option<Uuid>,
    pub search: Option<String>,
    pub limit: i64,
    pub before: Option<DateTime<Utc>>,
}

pub async fn query_logs(pool: &Pool, query: &LogQuery) -> Result<Vec<LogEntry>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    let mut sql = String::from(
        "SELECT id, project_id, host_id, timestamp, level, message, source, trace_id, span_id, fingerprint, attributes, stack_trace
         FROM log_entries WHERE project_id = $1"
    );
    let mut param_idx = 2u32;
    let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![Box::new(query.project_id)];

    if let Some(ref level) = query.level {
        sql.push_str(&format!(" AND level = ${}", param_idx));
        params.push(Box::new(level.clone()));
        param_idx += 1;
    }

    if let Some(ref host_id) = query.host_id {
        sql.push_str(&format!(" AND host_id = ${}", param_idx));
        params.push(Box::new(*host_id));
        param_idx += 1;
    }

    if let Some(ref search) = query.search {
        sql.push_str(&format!(" AND message ILIKE ${}", param_idx));
        params.push(Box::new(format!("%{}%", search)));
        param_idx += 1;
    }

    if let Some(ref before) = query.before {
        sql.push_str(&format!(" AND timestamp < ${}", param_idx));
        params.push(Box::new(*before));
        param_idx += 1;
    }

    let _ = param_idx; // suppress unused warning

    sql.push_str(" ORDER BY timestamp DESC LIMIT $");
    sql.push_str(&params.len().wrapping_add(1).to_string());
    params.push(Box::new(query.limit));

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    let rows = client.query(&sql, &param_refs).await?;

    Ok(rows.iter().map(|r| LogEntry {
        id: r.get(0),
        project_id: r.get(1),
        host_id: r.get(2),
        timestamp: r.get(3),
        level: r.get(4),
        message: r.get(5),
        source: r.get(6),
        trace_id: r.get(7),
        span_id: r.get(8),
        fingerprint: r.get(9),
        attributes: r.get(10),
        stack_trace: r.get(11),
    }).collect())
}

pub async fn query_logs_by_trace(pool: &Pool, trace_id: &str) -> Result<Vec<LogEntry>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT id, project_id, host_id, timestamp, level, message, source, trace_id, span_id, fingerprint, attributes, stack_trace
         FROM log_entries WHERE trace_id = $1 ORDER BY timestamp ASC",
        &[&trace_id],
    ).await?;

    Ok(rows.iter().map(|r| LogEntry {
        id: r.get(0),
        project_id: r.get(1),
        host_id: r.get(2),
        timestamp: r.get(3),
        level: r.get(4),
        message: r.get(5),
        source: r.get(6),
        trace_id: r.get(7),
        span_id: r.get(8),
        fingerprint: r.get(9),
        attributes: r.get(10),
        stack_trace: r.get(11),
    }).collect())
}
