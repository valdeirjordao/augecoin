//! Database layer for the AugeCoin Gateway
//!
//! PostgreSQL schema with migrations for:
//!   - developers
//!   - api_keys
//!   - api_usage (monthly counters)
//!   - billing_events (overage charges, subscriptions)
//!   - invoices
//!   - webhooks
//!   - payment_sessions

use sqlx::{postgres::PgPoolOptions, PgPool, Pool, Postgres};
use tracing::info;

pub type DbPool = Pool<Postgres>;

pub struct Database {
    pub pool: DbPool,
}

impl Database {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(2)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Run all pending migrations
    pub async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await?;
        info!("Database migrations applied.");
        Ok(())
    }
}
