pub mod api;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

pub fn maturity_interest(amount: Decimal, rate_pct: Decimal, term_months: i32) -> Decimal {
    amount * rate_pct / Decimal::from(100) * Decimal::from(term_months) / Decimal::from(12)
}

pub fn compute_maturity(start: NaiveDate, term_months: i32) -> NaiveDate {
    start.checked_add_months(chrono::Months::new(term_months as u32)).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { D7, D1, D0 }

pub fn tier_for(days_left: i64) -> Option<Tier> {
    match days_left {
        7 => Some(Tier::D7),
        1 => Some(Tier::D1),
        0 => Some(Tier::D0),
        _ => None,
    }
}

pub struct DepositLike {
    pub remind_7d: bool,
    pub remind_1d: bool,
    pub remind_today: bool,
    pub notified_7d: Option<NaiveDate>,
    pub notified_1d: Option<NaiveDate>,
    pub notified_today: Option<NaiveDate>,
    pub maturity_date: NaiveDate,
}

pub fn should_notify(d: &DepositLike, today: NaiveDate) -> Option<Tier> {
    let days = (d.maturity_date - today).num_days();
    match tier_for(days) {
        Some(Tier::D7) if d.remind_7d && d.notified_7d != Some(today) => Some(Tier::D7),
        Some(Tier::D1) if d.remind_1d && d.notified_1d != Some(today) => Some(Tier::D1),
        Some(Tier::D0) if d.remind_today && d.notified_today != Some(today) => Some(Tier::D0),
        _ => None,
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Deposit {
    pub id: i64,
    pub person: String,
    pub bank: String,
    pub currency: String,
    pub amount: Decimal,
    pub rate: Decimal,
    pub start_date: NaiveDate,
    pub term_months: i32,
    pub maturity_date: NaiveDate,
    pub remind_7d: bool,
    pub remind_1d: bool,
    pub remind_today: bool,
    pub notified_7d: Option<NaiveDate>,
    pub notified_1d: Option<NaiveDate>,
    pub notified_today: Option<NaiveDate>,
    pub note: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewDeposit {
    pub person: String,
    pub bank: String,
    #[serde(default = "d_cny")]
    pub currency: String,
    pub amount: Decimal,
    pub rate: Decimal,
    pub start_date: NaiveDate,
    pub term_months: i32,
    #[serde(default = "t")]
    pub remind_7d: bool,
    #[serde(default = "t")]
    pub remind_1d: bool,
    #[serde(default = "t")]
    pub remind_today: bool,
    pub note: Option<String>,
}

fn d_cny() -> String { "CNY".into() }
fn t() -> bool { true }

pub async fn create(pool: &PgPool, b: &NewDeposit) -> Result<i64, sqlx::Error> {
    let maturity = compute_maturity(b.start_date, b.term_months);
    sqlx::query_scalar(
        "INSERT INTO deposits (person, bank, currency, amount, rate, start_date, term_months, maturity_date, remind_7d, remind_1d, remind_today, note)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING id")
        .bind(&b.person).bind(&b.bank).bind(&b.currency).bind(b.amount).bind(b.rate)
        .bind(b.start_date).bind(b.term_months).bind(maturity)
        .bind(b.remind_7d).bind(b.remind_1d).bind(b.remind_today).bind(&b.note)
        .fetch_one(pool).await
}

pub async fn list(pool: &PgPool) -> Result<Vec<Deposit>, sqlx::Error> {
    sqlx::query_as::<_, Deposit>("SELECT * FROM deposits ORDER BY maturity_date").fetch_all(pool).await
}

pub async fn update(pool: &PgPool, id: i64, b: &NewDeposit) -> Result<bool, sqlx::Error> {
    let maturity = compute_maturity(b.start_date, b.term_months);
    let r = sqlx::query(
        "UPDATE deposits SET person=$2, bank=$3, currency=$4, amount=$5, rate=$6, start_date=$7,
                term_months=$8, maturity_date=$9, remind_7d=$10, remind_1d=$11, remind_today=$12,
                note=$13, updated_at=now()
         WHERE id=$1")
        .bind(id).bind(&b.person).bind(&b.bank).bind(&b.currency).bind(b.amount).bind(b.rate)
        .bind(b.start_date).bind(b.term_months).bind(maturity)
        .bind(b.remind_7d).bind(b.remind_1d).bind(b.remind_today).bind(&b.note)
        .execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM deposits WHERE id=$1").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CurrencySummary {
    pub currency: String,
    pub count: i64,
    pub total: Decimal,
    pub total_maturity: Decimal,
}

pub async fn summary(pool: &PgPool) -> Result<Vec<CurrencySummary>, sqlx::Error> {
    // PG 无 maturity_interest 函数，到期收益内联表达式（amount×rate%×月/12）
    sqlx::query_as::<_, CurrencySummary>(
        "SELECT currency, count(*) AS count, sum(amount) AS total,
                sum(amount + amount * rate / 100 * term_months / 12) AS total_maturity
         FROM deposits GROUP BY currency")
        .fetch_all(pool).await
}

pub async fn due_in_30d(pool: &PgPool) -> Result<Vec<Deposit>, sqlx::Error> {
    sqlx::query_as::<_, Deposit>(
        "SELECT * FROM deposits
         WHERE maturity_date <= (current_date + 30) AND maturity_date >= current_date
         ORDER BY maturity_date")
        .fetch_all(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn d(s: &str) -> Decimal { Decimal::from_str(s).unwrap() }
    fn date(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }

    #[test]
    fn interest_math() {
        // 10000 元年利率 2.5% 存 12 个月 → 250
        assert_eq!(maturity_interest(d("10000"), d("2.5"), 12), d("250"));
        // 10000 元年利率 2.0% 存 3 个月 → 50
        assert_eq!(maturity_interest(d("10000"), d("2.0"), 3), d("50"));
    }

    #[test]
    fn maturity_clamps_month_end() {
        assert_eq!(compute_maturity(date("2026-01-31"), 1), date("2026-02-28"));
        assert_eq!(compute_maturity(date("2026-11-10"), 36), date("2029-11-10"));
    }

    #[test]
    fn tier_only_at_7_1_0_days() {
        assert_eq!(tier_for(7), Some(Tier::D7));
        assert_eq!(tier_for(1), Some(Tier::D1));
        assert_eq!(tier_for(0), Some(Tier::D0));
        assert_eq!(tier_for(3), None);
        assert_eq!(tier_for(-1), None);
    }

    fn like(maturity: &str, _today: &str, flags: (bool, bool, bool), notified: Option<NaiveDate>) -> DepositLike {
        DepositLike {
            remind_7d: flags.0, remind_1d: flags.1, remind_today: flags.2,
            notified_7d: notified, notified_1d: notified, notified_today: notified,
            maturity_date: date(maturity),
        }
    }

    #[test]
    fn should_notify_respects_tier_flag_and_dedupe() {
        let today = date("2026-11-10");
        // 7 天档
        assert_eq!(should_notify(&like("2026-11-17", "2026-11-10", (true, true, true), None), today), Some(Tier::D7));
        // 档位关闭
        assert_eq!(should_notify(&like("2026-11-17", "2026-11-10", (false, true, true), None), today), None);
        // 当天已推过 → 不再推
        assert_eq!(should_notify(&like("2026-11-10", "2026-11-10", (true, true, true), Some(today)), today), None);
        // 隔天重推同一档允许（notified 仅记日期）
        assert_eq!(should_notify(&like("2026-11-11", "2026-11-10", (true, true, true), Some(date("2026-11-09"))), today), Some(Tier::D1));
    }
}
