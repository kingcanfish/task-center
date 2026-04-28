use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn pg_pool() -> Result<PgPool> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://task_center:task_center@localhost:5432/task_center".to_string()
    });
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    sqlx::migrate!("./db/migrations").run(&pool).await?;
    Ok(pool)
}
