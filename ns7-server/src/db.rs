use sqlx::postgres::{PgPool, PgPoolOptions};

/// Connects to Postgres and runs migrations.
///
/// Retries on failure because the server and database start together in the
/// compose stack — `depends_on` only waits for the container to start, not
/// for Postgres to finish initializing and accept connections.
pub async fn connect() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://nanostack7:devpassword@postgres:5432/nanostack7".to_string());

    const MAX_ATTEMPTS: u32 = 30;
    let mut attempt = 0;
    let pool = loop {
        attempt += 1;
        match PgPoolOptions::new().max_connections(5).connect(&url).await {
            Ok(pool) => break pool,
            Err(e) if attempt < MAX_ATTEMPTS => {
                tracing::warn!(attempt, error = %e, "database not ready yet, retrying in 2s");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => return Err(anyhow::anyhow!("could not connect to database after {attempt} attempts: {e}")),
        }
    };

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database connected and migrations applied");
    Ok(pool)
}
