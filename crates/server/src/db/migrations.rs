use deadpool_postgres::Pool;
use chrono::{Utc, Duration, NaiveDate};

pub async fn run_migrations(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    // Create migrations tracking table
    client.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )", &[]
    ).await?;

    let migrations: Vec<(&str, &str)> = vec![
        ("001_create_projects", include_str!("../../migrations/001_create_projects.sql")),
        ("002_create_hosts", include_str!("../../migrations/002_create_hosts.sql")),
        ("003_create_log_entries", include_str!("../../migrations/003_create_log_entries.sql")),
        ("004_create_spans", include_str!("../../migrations/004_create_spans.sql")),
        ("005_create_metrics", include_str!("../../migrations/005_create_metrics.sql")),
        ("006_create_error_groups", include_str!("../../migrations/006_create_error_groups.sql")),
    ];

    for (name, sql) in &migrations {
        let exists: bool = client.query_one(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = $1)",
            &[name],
        ).await?.get(0);

        if !exists {
            tracing::info!("Running migration: {}", name);
            client.batch_execute(sql).await?;
            client.execute(
                "INSERT INTO _migrations (name) VALUES ($1)",
                &[name],
            ).await?;
        }
    }

    // Create partitions for the next 7 days
    create_partitions(pool, 7).await?;

    tracing::info!("Migrations complete");
    Ok(())
}

pub async fn create_partitions(pool: &Pool, days_ahead: i64) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let today = Utc::now().date_naive();

    let partitioned_tables = vec!["log_entries", "spans", "metrics"];

    for day_offset in 0..days_ahead {
        let date = today + Duration::days(day_offset);
        let next_date = date + Duration::days(1);

        for table in &partitioned_tables {
            let partition_name = format!("{}_{}", table, date.format("%Y%m%d"));
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {} PARTITION OF {} FOR VALUES FROM ('{}') TO ('{}')",
                partition_name, table, date, next_date
            );
            if let Err(e) = client.execute(&sql, &[]).await {
                // Ignore "already exists" errors
                if !e.to_string().contains("already exists") {
                    tracing::error!("Failed to create partition {}: {}", partition_name, e);
                }
            }
        }
    }

    Ok(())
}

pub async fn drop_old_partitions(
    pool: &Pool,
    logs_days: u32,
    traces_days: u32,
    metrics_days: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let today = Utc::now().date_naive();

    let tables_with_retention: Vec<(&str, u32)> = vec![
        ("log_entries", logs_days),
        ("spans", traces_days),
        ("metrics", metrics_days),
    ];

    for (table, days) in &tables_with_retention {
        // Check for partitions older than retention period
        let cutoff = today - Duration::days(*days as i64);

        // List partitions by querying pg_inherits
        let rows = client.query(
            "SELECT c.relname FROM pg_inherits i
             JOIN pg_class c ON c.oid = i.inhrelid
             JOIN pg_class p ON p.oid = i.inhparent
             WHERE p.relname = $1
             ORDER BY c.relname",
            &[table],
        ).await?;

        for row in rows {
            let partition_name: &str = row.get(0);
            // Extract date from partition name (e.g., log_entries_20260301)
            if let Some(date_str) = partition_name.strip_prefix(&format!("{}_", table)) {
                if let Ok(partition_date) = NaiveDate::parse_from_str(date_str, "%Y%m%d") {
                    if partition_date < cutoff {
                        tracing::info!("Dropping old partition: {}", partition_name);
                        let sql = format!("DROP TABLE IF EXISTS {}", partition_name);
                        client.execute(&sql, &[]).await?;
                    }
                }
            }
        }
    }

    Ok(())
}
