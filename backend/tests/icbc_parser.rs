use chrono::{TimeZone, Utc};

#[test]
fn parses_icbc_fixture_rows() {
    let html = std::fs::read_to_string("tests/fixtures/icbc.html").unwrap();
    let pts = fintrack::sources::icbc::parse(&html, chrono::Utc::now()).unwrap();
    assert!(pts.iter().any(|p| p.pair == "USD/CNY" && p.source == "icbc"));
    assert!(pts.iter().any(|p| p.pair == "GBP/CNY"), "fixture 应含英镑行");
    assert!(pts.iter().all(|p| p.fx_bid.map_or(true, |v| v > rust_decimal::Decimal::ZERO)));
    assert!(pts.iter().all(|p| p.fx_ask.map_or(true, |v| v > rust_decimal::Decimal::ZERO)));
}

#[test]
fn rejects_empty_html() {
    assert!(fintrack::sources::icbc::parse("<html></html>", chrono::Utc::now()).is_err());
}

#[test]
fn icbc_time_converts_shanghai_to_utc() {
    // 2026.11.10 14:30:12 +08:00 == 06:30:12Z（工行风格点分格式）
    let t = fintrack::sources::icbc::parse_time("2026.11.10 14:30:12");
    let expect = Utc.with_ymd_and_hms(2026, 11, 10, 6, 30, 12).unwrap();
    assert_eq!(t, Some(expect));

    // 斜杠格式兼容（同 boc）
    let t2 = fintrack::sources::icbc::parse_time("2026/11/10 17:11:39");
    let expect2 = Utc.with_ymd_and_hms(2026, 11, 10, 9, 11, 39).unwrap();
    assert_eq!(t2, Some(expect2));
}

// fixture 为合成数据（线上端点待核验），不做精确数值 pinning：
// 仅断言结构行为 —— 页面时间优先于 default_now、非目标币种（加拿大元）被跳过、
// /100 归一后价格落在合理区间。
#[test]
fn uses_page_time_and_skips_non_target_currency() {
    let html = std::fs::read_to_string("tests/fixtures/icbc.html").unwrap();
    let default_now = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    let pts = fintrack::sources::icbc::parse(&html, default_now).unwrap();

    assert!(!pts.iter().any(|p| p.pair == "CAD/CNY"), "非目标币种行应被跳过");
    let usd = pts.iter().find(|p| p.pair == "USD/CNY").expect("应解析出美元");
    assert_ne!(usd.recorded_at, default_now, "发布时间应取页面值而非 default_now");
    // 705.32/100 = 7.0532 ∈ (1, 10)
    assert!(usd.fx_bid.is_some_and(|v| v > rust_decimal::Decimal::ONE && v < rust_decimal::Decimal::from(10)));
    assert!(usd.fx_ask > usd.fx_bid, "卖价应高于买价");
    assert_eq!(usd.mid, None, "工行牌价无折算价列，mid 应为 None");
}
