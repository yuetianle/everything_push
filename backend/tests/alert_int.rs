use chrono::Utc;
use fintrack::alerts::engine;
use fintrack::rates::RatePoint;
use rust_decimal::Decimal;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    fintrack::db::connect(&url).await.unwrap()
}

#[tokio::test]
#[ignore]
async fn evaluate_all_triggers_once_then_rearms() {
    let pool = pool().await;
    sqlx::query("DELETE FROM alert_rules").execute(&pool).await.unwrap();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO alert_rules (source, pair, price_type, direction, threshold)
         VALUES ('api','USD/CNY','mid','above',7.10) RETURNING id")
        .fetch_one(&pool).await.unwrap();

    let mk = |mid: Decimal| RatePoint {
        source: "api".into(), pair: "USD/CNY".into(),
        fx_bid: None, fx_ask: None, mid: Some(mid),
        recorded_at: Utc::now(), note: None,
    };

    let t1 = engine::evaluate_all(&pool, &mk(Decimal::new(711, 2)), Utc::now()).await.unwrap();
    assert_eq!(t1.len(), 1);
    assert_eq!(t1[0].rule_id, id);
    let t2 = engine::evaluate_all(&pool, &mk(Decimal::new(715, 2)), Utc::now()).await.unwrap();
    assert!(t2.is_empty(), "仍越限不重推");
    engine::evaluate_all(&pool, &mk(Decimal::new(700, 2)), Utc::now()).await.unwrap(); // 恢复→再武装
    let armed: bool = sqlx::query_scalar("SELECT armed FROM alert_rule_state WHERE rule_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert!(armed);
}
