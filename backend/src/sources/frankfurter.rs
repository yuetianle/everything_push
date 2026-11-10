use crate::rates::RatePoint;
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::str::FromStr;

pub const BASE_URL: &str = "https://api.frankfurter.dev/v1";

#[derive(Deserialize)]
struct LatestResp {
    date: NaiveDate,
    rates: HashMap<String, f64>,
}

#[derive(Deserialize)]
struct RangeResp {
    rates: HashMap<NaiveDate, HashMap<String, f64>>,
}

// brief 原文用 Decimal::from_f64_retain，但该函数保留 f64 二进制尾差
// （如 7.18 → "7.179999999999999715782905696"），与测试断言/业务展示不符；
// rust_decimal 自身 serde 的 visit_f64 即采用 from_str(&value.to_string())
// （最短往返表示），此处对齐该语义。
fn dec(v: f64) -> Decimal {
    Decimal::from_str(&v.to_string()).unwrap_or_default()
}

fn point(base: &str, date: NaiveDate, cny: f64) -> RatePoint {
    RatePoint {
        source: "api".into(),
        pair: format!("{base}/CNY"),
        fx_bid: None,
        fx_ask: None,
        mid: Some(dec(cny)),
        // brief 原文 NaiveDateTime::from_timestamp_opt(date.and_hms_opt(...), 0) 类型不匹配
        // （该函数入参是 epoch 秒），直接取当日 09:00 UTC 语义等价。
        recorded_at: date.and_hms_opt(9, 0, 0).unwrap().and_utc(),
        note: None,
    }
}

/// base_url 可注入（测试），生产传 BASE_URL
pub async fn fetch_latest_at(client: &reqwest::Client, base_url: &str, base: &str) -> Result<RatePoint, String> {
    let url = format!("{base_url}/latest?base={base}&symbols=CNY");
    let r: LatestResp = client.get(&url).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    let cny = r.rates.get("CNY").copied().ok_or("响应缺少 CNY")?;
    Ok(point(base, r.date, cny))
}

pub async fn fetch_latest(client: &reqwest::Client, base: &str) -> Result<RatePoint, String> {
    fetch_latest_at(client, BASE_URL, base).await
}

pub async fn fetch_range_at(
    client: &reqwest::Client, base_url: &str, base: &str, from: NaiveDate, to: NaiveDate,
) -> Result<Vec<RatePoint>, String> {
    let url = format!("{base_url}/{from}..{to}?base={base}&symbols=CNY");
    let r: RangeResp = client.get(&url).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    let mut pts: Vec<RatePoint> = r.rates.into_iter()
        .filter_map(|(d, m)| m.get("CNY").map(|v| point(base, d, *v)))
        .collect();
    pts.sort_by_key(|p| p.recorded_at);
    Ok(pts)
}

pub async fn fetch_range(client: &reqwest::Client, base: &str, from: NaiveDate, to: NaiveDate) -> Result<Vec<RatePoint>, String> {
    fetch_range_at(client, BASE_URL, base, from, to).await
}

/// 幂等入库：每对货币从已存最大日期的次日开始增量拉取；首次回填 365 天。返回新增行数。
pub async fn ingest_api_rates(pool: &PgPool, client: &reqwest::Client, pairs: &[String]) -> Result<usize, String> {
    let today = Utc::now().date_naive();
    let mut total = 0usize;
    for pair in pairs {
        let base = pair.split('/').next().unwrap();
        let max: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(recorded_at) FROM exchange_rates WHERE source='api' AND pair=$1")
            .bind(pair).fetch_optional(pool).await.map_err(|e| e.to_string())?
            .flatten();
        let from = max.map(|m| m.date_naive() + chrono::Duration::days(1))
            .unwrap_or(today - chrono::Duration::days(365));
        if from > today {
            continue;
        }
        let pts = fetch_range(client, base, from, today).await?;
        total += crate::rates::store::insert_many(pool, &pts).await.map_err(|e| e.to_string())? as usize;
    }
    Ok(total)
}
