use chrono::{Duration, Utc};
use fintrack::rates::{store, RatePoint};
use rust_decimal::Decimal;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    fintrack::db::connect(&url).await.unwrap()
}

async fn seed(pool: &PgPool) {
    sqlx::query("DELETE FROM exchange_rates").execute(pool).await.unwrap();
    let now = Utc::now();
    let pts: Vec<RatePoint> = (0i64..5)
        .map(|i| RatePoint {
            source: "boc".into(),
            pair: "USD/CNY".into(),
            fx_bid: Some(Decimal::new(7100 + i, 3)),
            fx_ask: Some(Decimal::new(7300 + i, 3)),
            mid: None,
            recorded_at: now - Duration::minutes(i * 10),
            note: None,
        })
        .collect();
    store::insert_many(pool, &pts).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn latest_returns_distinct_source_pair() {
    let pool = pool().await;
    seed(&pool).await;
    let rows = store::latest(&pool).await.unwrap();
    let usd_boc = rows.iter().find(|r| r.pair == "USD/CNY" && r.source == "boc").unwrap();
    assert_eq!(usd_boc.fx_bid, Some(Decimal::new(7100, 3)), "最新一条");
}

#[tokio::test]
#[ignore]
async fn history_buckets_points() {
    let pool = pool().await;
    seed(&pool).await;
    let now = Utc::now();
    let rows = store::history(
        &pool, "USD/CNY", &["boc".into()], now - Duration::hours(1), now + Duration::minutes(1), 6,
    ).await.unwrap();
    // 50 分钟跨度、桶下限 60s → 5 点收进 5~6 个桶
    assert!(rows.len() >= 4 && rows.len() <= 6, "got {}", rows.len());
}
