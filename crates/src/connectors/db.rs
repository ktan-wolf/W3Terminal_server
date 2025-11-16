use crate::connectors::state::PriceUpdate;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row, postgres::PgRow};

#[derive(Debug, Serialize)]
pub struct HistoricalPrice {
    pub source: String,
    pub pair: String,
    pub price: f64,
    pub timestamp: DateTime<Utc>,
}

/// Initialize Postgres / TimescaleDB pool
pub async fn init_db(database_url: &str) -> PgPool {
    let pool = PgPool::connect(database_url)
        .await
        .expect("Failed to connect to Postgres");
    // Optional: create table if not exists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS price_history (
            id SERIAL PRIMARY KEY,
            source TEXT NOT NULL,
            pair TEXT NOT NULL,
            price DOUBLE PRECISION NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT now()
        )
    "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create price_history table");
    pool
}

// Insert
pub async fn insert_price(pool: &PgPool, update: &PriceUpdate) {
    let _ = sqlx::query("INSERT INTO price_history (source, pair, price) VALUES ($1, $2, $3)")
        .bind(&update.source)
        .bind(&update.pair)
        .bind(update.price)
        .execute(pool)
        .await;
}

// Fetch
pub async fn fetch_historical(pool: &PgPool, pair: &str) -> Vec<HistoricalPrice> {
    let rows = sqlx::query(
        "SELECT source, pair, price, timestamp FROM price_history WHERE pair = $1 ORDER BY timestamp ASC"
    )
    .bind(pair)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| HistoricalPrice {
            source: r.try_get("source").unwrap_or_default(),
            pair: r.try_get("pair").unwrap_or_default(),
            price: r.try_get("price").unwrap_or(0.0),
            timestamp: r
                .try_get("timestamp")
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
        .collect()
}
