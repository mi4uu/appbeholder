use deadpool_postgres::Pool;
use uuid::Uuid;

pub async fn get_or_create_project(pool: &Pool, slug: &str) -> Result<Uuid, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    // Try to find existing project
    let row = client.query_opt(
        "SELECT id FROM projects WHERE slug = $1",
        &[&slug],
    ).await?;

    if let Some(row) = row {
        return Ok(row.get(0));
    }

    // Create new project (use slug as name too)
    let name = slug.replace('-', " ");
    let row = client.query_one(
        "INSERT INTO projects (name, slug) VALUES ($1, $2)
         ON CONFLICT (slug) DO UPDATE SET slug = EXCLUDED.slug
         RETURNING id",
        &[&name, &slug],
    ).await?;

    tracing::info!("Auto-created project: {} ({})", slug, row.get::<_, Uuid>(0));
    Ok(row.get(0))
}

pub async fn get_or_create_host(pool: &Pool, project_id: Uuid, hostname: &str) -> Result<Uuid, Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    let row = client.query_one(
        "INSERT INTO hosts (project_id, hostname)
         VALUES ($1, $2)
         ON CONFLICT (project_id, hostname) DO UPDATE SET last_seen = NOW()
         RETURNING id",
        &[&project_id, &hostname],
    ).await?;

    Ok(row.get(0))
}

pub async fn list_projects(pool: &Pool) -> Result<Vec<(Uuid, String, String)>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT id, name, slug FROM projects ORDER BY name",
        &[],
    ).await?;

    Ok(rows.iter().map(|r| (r.get(0), r.get(1), r.get(2))).collect())
}

pub async fn list_hosts(pool: &Pool, project_id: Uuid) -> Result<Vec<(Uuid, String)>, Box<dyn std::error::Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        "SELECT id, hostname FROM hosts WHERE project_id = $1 ORDER BY hostname",
        &[&project_id],
    ).await?;

    Ok(rows.iter().map(|r| (r.get(0), r.get(1))).collect())
}
