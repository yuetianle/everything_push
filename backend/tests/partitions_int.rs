use sqlx::PgPool;
use fintrack::db;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    db::connect(&url).await.unwrap()
}

#[tokio::test]
#[ignore]
async fn ensure_partitions_idempotent_and_old_partition_dropped() {
    let pool = pool().await;
    sqlx::migrate!().run(&pool).await.unwrap();
    db::ensure_partitions(&pool).await.unwrap();
    db::ensure_partitions(&pool).await.unwrap(); // 幂等

    // 造一个 2024-01 老分区并写入数据
    db::ensure_partition_for(&pool, 2024, 1).await.unwrap();
    sqlx::query("INSERT INTO exchange_rates (source,pair,mid,recorded_at) VALUES ('api','USD/CNY',7.1,'2024-01-15T00:00:00Z')")
        .execute(&pool).await.unwrap();

    let dropped = db::drop_old_partitions(&pool, 24).await.unwrap();
    assert!(dropped.contains(&"exchange_rates_202401".to_string()));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM exchange_rates WHERE recorded_at < '2024-02-01'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0); // 分区删除后数据随分区消失
}
