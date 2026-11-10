use chrono::{TimeZone, Utc};

#[test]
fn parses_usd_and_aud_rows() {
    let html = std::fs::read_to_string("tests/fixtures/boc.html").unwrap();
    let now = Utc::now();
    let pts = fintrack::sources::boc::parse(&html, now).unwrap();
    let usd = pts.iter().find(|p| p.pair == "USD/CNY").expect("应解析出美元");
    assert_eq!(usd.source, "boc");
    assert!(usd.fx_bid.is_some_and(|v| v > rust_decimal::Decimal::new(600, 2)), "现汇买入价应>6.00");
    assert!(usd.fx_ask.is_some_and(|v| v > rust_decimal::Decimal::new(600, 2)));
    let aud = pts.iter().find(|p| p.pair == "AUD/CNY");
    assert!(aud.is_some(), "fixture 应含澳元行");
}

#[test]
fn rejects_html_without_rows() {
    let r = fintrack::sources::boc::parse("<html><body>维护中</body></html>", Utc::now());
    assert!(r.is_err());
}

#[test]
fn boc_time_converts_shanghai_to_utc() {
    // 2026.11.10 14:30:12 +08:00 == 06:30:12Z
    let t = fintrack::sources::boc::parse_time("2026.11.10 14:30:12");
    let expect = Utc.with_ymd_and_hms(2026, 11, 10, 6, 30, 12).unwrap();
    assert_eq!(t, Some(expect));
}

// 真实页面（fixture）发布日期为斜杠格式 2026/11/10 17:11:39，且美元现汇买入 665.67
// 为 100 美元单位 → 归一后应为 6.6567，时间应取页面值而非 default_now。
#[test]
fn parses_real_page_slash_time_and_scales_per_100() {
    let t = fintrack::sources::boc::parse_time("2026/11/10 17:11:39");
    let expect = Utc.with_ymd_and_hms(2026, 11, 10, 9, 11, 39).unwrap();
    assert_eq!(t, Some(expect), "斜杠日期格式应按 Asia/Shanghai 转 Utc");

    let html = std::fs::read_to_string("tests/fixtures/boc.html").unwrap();
    let default_now = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    let pts = fintrack::sources::boc::parse(&html, default_now).unwrap();
    let usd = pts.iter().find(|p| p.pair == "USD/CNY").expect("应解析出美元");
    assert_eq!(usd.recorded_at, expect, "发布时间应取页面值而非 default_now");
    assert_eq!(usd.fx_bid, Some(rust_decimal::Decimal::new(66567, 4)), "665.67/100 应为 6.6567");
    assert_eq!(usd.fx_ask, Some(rust_decimal::Decimal::new(66847, 4)), "668.47/100 应为 6.6847");
    assert_eq!(usd.mid, Some(rust_decimal::Decimal::new(67167, 4)), "671.67/100 应为 6.7167");
}
