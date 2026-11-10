use crate::rates::RatePoint;
use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Asia::Shanghai;
use rust_decimal::Decimal;
use scraper::{Html, Selector};

// ⚠️ 线上端点待核验（2026-11-10 实测，均不可用）：
//   候选1 https://www.icbc.com.cn/ICBCDynamicSite2/Charts/SiteDefault.aspx?Type=36 → 返回 404 页
//   候选2 https://www.icbc.com.cn/column/1438058341489590609.html → 返回 404 页（JS 跳回首页）
//   附加  https://www.icbc.com.cn/column/1438058341489590611.html → 同样 404
// 工行官网牌价页为 JS 渲染，静态 HTML 不含数据。fetch() 暂直接返回 Err（见下），
// 健康跟踪（sources::health）会将其标记为失效源，系统其余部分不受影响。
// 待核验出真实可用端点后：恢复 fetch 实现、以真实页面替换 tests/fixtures/icbc.html
// 并按实际列序复核 COL_* 常量。

// 工行牌价列序（0 基，按 fixture）：币种 | 现汇买入价 | 现钞买入价 | 现汇卖出价 | 现钞卖出价 | 发布时间
const COL_BID: usize = 1;
const COL_ASK: usize = 3;
const COL_TIME: usize = 5;

const NAME2PAIR: &[(&str, &str)] = &[
    ("美元", "USD"), ("欧元", "EUR"), ("日元", "JPY"),
    ("港币", "HKD"), ("英镑", "GBP"),
    // 工行页面澳币行名为「澳大利亚元」，同时保留「澳元」以兼容简称
    ("澳大利亚元", "AUD"), ("澳元", "AUD"),
];

pub const PAGE_URL: &str = "https://www.icbc.com.cn/column/1438058341489590609.html";

/// 线上端点待核验（尝试记录见文件头注释），暂不做网络请求，直接报错。
pub async fn fetch(_client: &reqwest::Client) -> Result<String, String> {
    Err("icbc 端点待核验".into())
}

/// 解析发布时间并按 Asia/Shanghai (+08:00) 转 Utc。
/// 工行页面为点分格式 `2026.11.10 14:30:12`，兼容斜杠格式 `2026/11/10 17:11:39`。
pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    ["%Y.%m.%d %H:%M:%S", "%Y/%m/%d %H:%M:%S"]
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
        let (bid, ask) = (dec(COL_BID), dec(COL_ASK));
        if bid.is_none() && ask.is_none() {
            continue;
        }
        let recorded_at = parse_time(&cells[COL_TIME]).unwrap_or(default_now);
        // 工行牌价为 100 外币兑人民币 → 换算为 1 外币；工行无折算价列，mid 置 None
        let scale = |d: Option<Decimal>| d.map(|d| d / Decimal::from(100));
        out.push(RatePoint {
            source: "icbc".into(),
            pair: format!("{code}/CNY"),
            fx_bid: scale(bid),
            fx_ask: scale(ask),
            mid: None,
            recorded_at,
            note: None,
        });
    }
    if out.is_empty() {
        Err("ICBC 解析失败：未匹配到任何币种行".into())
    } else {
        Ok(out)
    }
}
