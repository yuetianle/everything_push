#[tokio::test]
#[ignore]
async fn scan_marks_and_dedupes_same_day() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM deposits").execute(&pool).await.unwrap();

    let today = chrono::Utc::now().date_naive();
    let mat = today + chrono::Duration::days(7);
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO deposits (person,bank,currency,amount,rate,start_date,term_months,maturity_date)
         VALUES ('张三','中行','CNY',10000,2.5,$1,12,$2) RETURNING id")
        .bind(today - chrono::Duration::days(30)).bind(mat)
        .fetch_one(&pool).await.unwrap();

    let n1 = fintrack::maturity::scan_and_mark(&pool, today).await.unwrap();
    assert_eq!(n1.len(), 1);
    let n2 = fintrack::maturity::scan_and_mark(&pool, today).await.unwrap();
    assert!(n2.is_empty(), "同日不重复推送");
    let notified: chrono::NaiveDate = sqlx::query_scalar("SELECT notified_7d FROM deposits WHERE id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(notified, today);
}
