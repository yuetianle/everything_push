use fintrack::sources::frankfurter;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// 注：base_url 注入需含 /v1 前缀（与生产 BASE_URL 语义一致），mock 路径才能匹配 /v1/...。
#[tokio::test]
async fn parses_latest() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/latest"))
        .and(query_param("base", "USD")).and(query_param("symbols", "CNY"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_string(r#"{"amount":1.0,"base":"USD","date":"2026-11-09","rates":{"CNY":7.1825}}"#))
        .mount(&server).await;
    let c = reqwest::Client::new();
    let pt = frankfurter::fetch_latest_at(&c, &format!("{}/v1", server.uri()), "USD").await.unwrap();
    assert_eq!(pt.pair, "USD/CNY");
    assert_eq!(pt.source, "api");
    assert_eq!(pt.mid.unwrap().to_string(), "7.1825");
    assert_eq!(pt.recorded_at.date_naive().to_string(), "2026-11-09");
}

#[tokio::test]
async fn parses_range() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/2026-11-01..2026-11-03"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rates":{"2026-11-02":{"CNY":7.18},"2026-11-03":{"CNY":7.19}}}"#))
        .mount(&server).await;
    let c = reqwest::Client::new();
    let pts = frankfurter::fetch_range_at(&c, &format!("{}/v1", server.uri()), "USD",
        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()).await.unwrap();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0].mid.unwrap().to_string(), "7.18");
    assert_eq!(pts[1].recorded_at.date_naive().to_string(), "2026-11-03");
}

/// 集成测试：真实 frankfurter.dev + 真实 PG（TEST_DATABASE_URL）。
/// R6：若本机网络不可达（超时/DNS 失败），允许跳过，但必须在任务报告中
/// 原文引用报错输出 —— 不得静默删除本测试。
///
/// 回归覆盖分区修复：不再由测试预建分区（旧写法掩盖了生产路径缺陷），
/// 而是先 DROP 回填窗口内的月度分区（连同数据），ingest_api_rates
/// 必须自行补建分区，否则首批插入即 "no partition of relation" 失败。
#[tokio::test]
#[ignore]
async fn ingest_is_idempotent() {
    use chrono::{Datelike, Months};
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let today = chrono::Utc::now().date_naive();
    let this_month = today.with_day(1).unwrap();
    let mut cur = this_month.checked_sub_months(Months::new(13)).unwrap();
    while cur <= this_month {
        let name = format!("exchange_rates_{}{:02}", cur.year(), cur.month());
        sqlx::query(&format!("DROP TABLE IF EXISTS {name}")).execute(&pool).await.unwrap();
        cur = cur.checked_add_months(Months::new(1)).unwrap();
    }
    sqlx::query("DELETE FROM exchange_rates WHERE source='api'").execute(&pool).await.unwrap();
    let c = reqwest::Client::new();
    let n1 = frankfurter::ingest_api_rates(&pool, &c, &["USD/CNY".into()]).await.unwrap();
    assert!(n1 > 200, "首次回填约一年交易日, got {n1}");
    let n2 = frankfurter::ingest_api_rates(&pool, &c, &["USD/CNY".into()]).await.unwrap();
    assert!(n2 <= 1, "二次拉取几乎无新增, got {n2}");
}
