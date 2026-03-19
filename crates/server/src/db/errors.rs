use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use uuid::Uuid;

#[allow(dead_code)]
pub struct ErrorGroup {
    pub id: Uuid,
    pub project_id: Uuid,
    pub fingerprint: String,
    pub message: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub count: i64,
    pub status: String,
    pub hosts: Vec<String>,
}

pub struct ErrorGroupQuery {
    pub project_id: Uuid,
    pub status: Option<String>,
    pub search: Option<String>,
    pub host: Option<String>,
    pub limit: i64,
}

pub async fn upsert_error_group(
    pool: &Pool,
    project_id: Uuid,
    fingerprint: &str,
    message: &str,
    hostname: &str,
    timestamp: DateTime<Utc>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    client.execute(
        "INSERT INTO error_groups (project_id, fingerprint, message, first_seen, last_seen, count, status, hosts)
         VALUES ($1, $2, $3, $4, $4, 1, 'active', ARRAY[$5])
         ON CONFLICT (project_id, fingerprint) DO UPDATE SET
           count = error_groups.count + 1,
           last_seen = GREATEST(error_groups.last_seen, $4),
           message = $3,
           status = CASE WHEN error_groups.status = 'resolved' THEN 'active' ELSE error_groups.status END,
           hosts = CASE
             WHEN $5 = ANY(error_groups.hosts) THEN error_groups.hosts
             ELSE array_append(error_groups.hosts, $5)
           END",
        &[&project_id, &fingerprint, &message, &timestamp, &hostname],
    ).await?;
    Ok(())
}

pub async fn query_error_groups(
    pool: &Pool,
    query: &ErrorGroupQuery,
) -> Result<Vec<ErrorGroup>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    let mut sql = String::from(
        "SELECT id, project_id, fingerprint, message, first_seen, last_seen, count, status, hosts
         FROM error_groups WHERE project_id = $1"
    );
    let mut param_idx = 2u32;
    let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![Box::new(query.project_id)];

    if let Some(ref status) = query.status {
        sql.push_str(&format!(" AND status = ${}", param_idx));
        params.push(Box::new(status.clone()));
        param_idx += 1;
    }

    if let Some(ref search) = query.search {
        sql.push_str(&format!(" AND message ILIKE ${}", param_idx));
        params.push(Box::new(format!("%{}%", search)));
        param_idx += 1;
    }

    if let Some(ref host) = query.host {
        sql.push_str(&format!(" AND ${} = ANY(hosts)", param_idx));
        params.push(Box::new(host.clone()));
        param_idx += 1;
    }

    let _ = param_idx;

    sql.push_str(" ORDER BY last_seen DESC LIMIT $");
    sql.push_str(&params.len().wrapping_add(1).to_string());
    params.push(Box::new(query.limit));

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        params.iter().map(|p| &**p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    let rows = client.query(&sql, &param_refs).await?;

    Ok(rows.iter().map(|r| ErrorGroup {
        id: r.get(0),
        project_id: r.get(1),
        fingerprint: r.get(2),
        message: r.get(3),
        first_seen: r.get(4),
        last_seen: r.get(5),
        count: r.get(6),
        status: r.get(7),
        hosts: r.get(8),
    }).collect())
}

pub async fn update_error_group_status(
    pool: &Pool,
    group_id: Uuid,
    status: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    client.execute(
        "UPDATE error_groups SET status = $2 WHERE id = $1",
        &[&group_id, &status],
    ).await?;
    Ok(())
}

pub async fn get_error_group_sparkline(
    pool: &Pool,
    project_id: Uuid,
    fingerprint: &str,
) -> Result<Vec<i64>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT COALESCE(c.cnt, 0) AS cnt
         FROM generate_series(
           date_trunc('hour', NOW() - INTERVAL '23 hours'),
           date_trunc('hour', NOW()),
           INTERVAL '1 hour'
         ) AS bucket(ts)
         LEFT JOIN (
           SELECT date_trunc('hour', timestamp) AS hour, COUNT(*) AS cnt
           FROM log_entries
           WHERE project_id = $1 AND fingerprint = $2
             AND timestamp >= NOW() - INTERVAL '24 hours'
           GROUP BY hour
         ) c ON c.hour = bucket.ts
         ORDER BY bucket.ts",
        &[&project_id, &fingerprint],
    ).await?;

    Ok(rows.iter().map(|r| r.get::<_, i64>(0)).collect())
}

pub async fn query_error_group_entries(
    pool: &Pool,
    project_id: Uuid,
    fingerprint: &str,
    limit: i64,
) -> Result<Vec<crate::db::logs::LogEntry>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT id, project_id, host_id, timestamp, level, message, source, trace_id, span_id, fingerprint, attributes, stack_trace
         FROM log_entries
         WHERE project_id = $1 AND fingerprint = $2
         ORDER BY timestamp DESC
         LIMIT $3",
        &[&project_id, &fingerprint, &limit],
    ).await?;

    Ok(rows.iter().map(|r| crate::db::logs::LogEntry {
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
