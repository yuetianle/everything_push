pub mod api;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RatePoint {
    pub source: String,
    pub pair: String,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
    pub mid: Option<Decimal>,
    pub recorded_at: DateTime<Utc>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LatestRate {
    pub pair: String,
    pub source: String,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
    pub mid: Option<Decimal>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BucketPoint {
    pub source: String,
    pub t: DateTime<Utc>,
    pub mid: Option<Decimal>,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
}

pub mod store {
    use super::*;
    use sqlx::PgPool;

    pub async fn insert_many(pool: &PgPool, pts: &[RatePoint]) -> Result<u64, sqlx::Error> {
        let mut n = 0;
        for p in pts {
            let r = sqlx::query(
                "INSERT INTO exchange_rates (source, pair, fx_bid, fx_ask, mid, recorded_at, note)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&p.source).bind(&p.pair).bind(p.fx_bid).bind(p.fx_ask)
            .bind(p.mid).bind(p.recorded_at).bind(&p.note)
            .execute(pool).await?;
            n += r.rows_affected();
        }
        Ok(n)
    }

    pub async fn latest(pool: &PgPool) -> Result<Vec<LatestRate>, sqlx::Error> {
        sqlx::query_as(
            "SELECT DISTINCT ON (pair, source) pair, source, fx_bid, fx_ask, mid, recorded_at
             FROM exchange_rates ORDER BY pair, source, recorded_at DESC",
        )
        .fetch_all(pool).await
    }

    pub fn bucket_secs(from: DateTime<Utc>, to: DateTime<Utc>, max_points: u32) -> i64 {
        let range = (to - from).num_seconds().max(1);
        (range / max_points.max(1) as i64).max(60)
    }

    pub async fn history(
        pool: &PgPool, pair: &str, sources: &[String],
        from: DateTime<Utc>, to: DateTime<Utc>, max_points: u32,
    ) -> Result<Vec<BucketPoint>, sqlx::Error> {
        let b = bucket_secs(from, to, max_points);
        sqlx::query_as(
            "SELECT source,
                    to_timestamp(floor(extract(epoch FROM recorded_at) / $3) * $3) AS t,
                    avg(mid) AS mid, avg(fx_bid) AS fx_bid, avg(fx_ask) AS fx_ask
             FROM exchange_rates
             WHERE pair = $1 AND source = ANY($2) AND recorded_at >= $4 AND recorded_at < $5
             GROUP BY source, t
             ORDER BY source, t",
        )
        .bind(pair).bind(sources).bind(b).bind(from).bind(to)
        .fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bucket_secs_has_floor_and_scales() {
        use chrono::Duration;
        let now = chrono::Utc::now();
        assert_eq!(super::store::bucket_secs(now, now + Duration::seconds(10), 300), 60);
        let day = super::store::bucket_secs(now, now + Duration::hours(24), 288);
        assert!(day >= 300, "24h/288点 → 桶≥300s");
    }
}
