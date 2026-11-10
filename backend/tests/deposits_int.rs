use std::str::FromStr;
use rust_decimal::Decimal;

#[tokio::test]
#[ignore]
async fn crud_and_summary() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM deposits").execute(&pool).await.unwrap();

    let id = fintrack::deposits::create(&pool, &fintrack::deposits::NewDeposit {
        person: "张三".into(), bank: "中行".into(), currency: "CNY".into(),
        amount: Decimal::from_str("10000").unwrap(), rate: Decimal::from_str("2.5").unwrap(),
        start_date: chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(), term_months: 12,
        remind_7d: true, remind_1d: true, remind_today: true, note: None,
    }).await.unwrap();
    assert!(id > 0);

    let s = fintrack::deposits::summary(&pool).await.unwrap();
    let cny = s.iter().find(|x| x.currency == "CNY").unwrap();
    assert_eq!(cny.count, 1);
    assert_eq!(cny.total_maturity, Decimal::from_str("10250").unwrap()); // 10000+250
}
