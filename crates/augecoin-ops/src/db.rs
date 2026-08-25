//! PostgreSQL pool + migrations.

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;
use crate::Result;

pub static MIGRATOR: Migrator = sqlx::migrate!();

/// Create a connection pool and apply pending migrations.
pub async fn connect(config: &Config) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.database_url)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
