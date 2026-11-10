use crate::rates::RatePoint;
use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Asia::Shanghai;
use rust_decimal::Decimal;
use scraper::{Html, Selector};

// 中行牌价列序（0 基）：币种名 | 现汇买入 | 现钞买入 | 现汇卖出 | 现钞卖出 | 中行折算价 | 发布时间
const COL_BID: usize = 1;
const COL_ASK: usize = 3;
const COL_MID: usize = 5;
const COL_TIME: usize = 6;

const NAME2PAIR: &[(&str, &str)] = &[
    ("美元", "USD"), ("欧元", "EUR"), ("日元", "JPY"),
    ("港币", "HKD"), ("英镑", "GBP"),
    // 真实页面澳币行名为「澳大利亚元」，同时保留「澳元」以兼容简称
    ("澳大利亚元", "AUD"), ("澳元", "AUD"),
];

pub const PAGE_URL: &str = "https://www.boc.cn/sourcedb/whpj/";

pub async fn fetch(client: &reqwest::Client) -> Result<String, String> {
    client.get(PAGE_URL)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .text().await.map_err(|e| e.to_string())
}

/// 解析发布时间并按 Asia/Shanghai (+08:00) 转 Utc。
/// 真实页面为斜杠格式 `2026/11/10 17:11:39`，兼容点分格式 `2026.11.10 14:30:12`。
pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    ["%Y/%m/%d %H:%M:%S", "%Y.%m.%d %H:%M:%S"]
        .iter()
        .find_map(|fmt| {
            NaiveDateTime::parse_from_str(s, fmt)
                .ok()?
                .and_local_timezone(Shanghai)
                .single()
                .map(|dt| dt.with_timezone(&Utc))
        })
}

pub fn parse(html: &str, default_now: DateTime<Utc>) -> Result<Vec<RatePoint>, String> {
    let doc = Html::parse_document(html);
    let tr = Selector::parse("tr").unwrap();
    let td = Selector::parse("td").unwrap();
    let mut out = vec![];
    for row in doc.select(&tr) {
        let cells: Vec<String> = row.select(&td)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();
        if cells.len() <= COL_TIME {
            continue;
        }
        let Some(code) = NAME2PAIR.iter().find(|(n, _)| cells[0].contains(n)).map(|(_, c)| *c) else {
            continue;
        };
        let dec = |i: usize| cells.get(i).and_then(|s| s.parse::<Decimal>().ok());
        let (bid, ask, mid) = (dec(COL_BID), dec(COL_ASK), dec(COL_MID));
        if bid.is_none() && ask.is_none() && mid.is_none() {
            continue;
        }
        let recorded_at = parse_time(&cells[COL_TIME]).unwrap_or(default_now);
        // 中行牌价为 100 外币兑人民币 → 换算为 1 外币
        let scale = |d: Option<Decimal>| d.map(|d| d / Decimal::from(100));
        out.push(RatePoint {
            source: "boc".into(),
            pair: format!("{code}/CNY"),
            fx_bid: scale(bid),
            fx_ask: scale(ask),
            mid: scale(mid),
            recorded_at,
            note: None,
        });
    }
    if out.is_empty() {
        Err("BOC 解析失败：未匹配到任何币种行".into())
    } else {
        Ok(out)
    }
}
