use crate::rates::RatePoint;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction { Above, Below }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PriceType { FxBid, FxAsk, Mid }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision { Trigger, Rearm, Silent }

pub struct Rule {
    pub id: i64,
    pub direction: Direction,
    pub threshold: Decimal,
    pub cooldown_minutes: i64,
}

pub struct RuleState {
    pub armed: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
}

pub fn evaluate(rule: &Rule, state: &RuleState, value: Decimal, now: DateTime<Utc>) -> Decision {
    let breached = match rule.direction {
        Direction::Above => value >= rule.threshold,
        Direction::Below => value <= rule.threshold,
    };
    if !breached {
        return Decision::Rearm;
    }
    let cooldown_elapsed = state
        .last_triggered_at
        .map_or(true, |t| now.signed_duration_since(t) >= Duration::minutes(rule.cooldown_minutes));
    if state.armed && cooldown_elapsed {
        Decision::Trigger
    } else {
        Decision::Silent
    }
}

#[derive(Debug, Serialize)]
pub struct TriggeredRule {
    pub rule_id: i64,
    pub source: String,
    pub pair: String,
    pub price_type: String,
    pub direction: String,
    pub threshold: Decimal,
    pub value: Decimal,
}

impl TriggeredRule {
    pub fn message(&self) -> String {
        let dir = if self.direction == "above" { "上破" } else { "下破" };
        let field = match self.price_type.as_str() {
            "fx_bid" => "现汇买入价",
            "fx_ask" => "现汇卖出价",
            _ => "中间价",
        };
        format!("{} {}[{}] {} 阈值 {}，当前 {}",
            self.pair, self.source, field, dir, self.threshold, self.value)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JoinedRow {
    id: i64,
    source: String,
    pair: String,
    price_type: String,
    direction: String,
    threshold: Decimal,
    cooldown_minutes: i32, // DB 列为 int(INT4)，sqlx 严格类型检查不兼容 i64
    armed: Option<bool>,
    last_triggered_at: Option<DateTime<Utc>>,
}

/// 收到新数据点时评估所有匹配的启用规则；写回状态并返回触发的规则
pub async fn evaluate_all(pool: &PgPool, pt: &RatePoint, now: DateTime<Utc>) -> Result<Vec<TriggeredRule>, sqlx::Error> {
    let rows: Vec<JoinedRow> = sqlx::query_as(
        "SELECT r.id, r.source, r.pair, r.price_type, r.direction, r.threshold, r.cooldown_minutes,
                s.armed, s.last_triggered_at
         FROM alert_rules r
         LEFT JOIN alert_rule_state s ON s.rule_id = r.id
         WHERE r.enabled AND r.source = $1 AND r.pair = $2")
        .bind(&pt.source).bind(&pt.pair)
        .fetch_all(pool).await?;

    let mut triggered = vec![];
    for row in rows {
        let value = match row.price_type.as_str() {
            "fx_bid" => pt.fx_bid,
            "fx_ask" => pt.fx_ask,
            _ => pt.mid,
        };
        let Some(value) = value else { continue };
        let direction = if row.direction == "above" { Direction::Above } else { Direction::Below };
        let rule = Rule {
            id: row.id,
            direction,
            threshold: row.threshold,
            cooldown_minutes: row.cooldown_minutes as i64,
        };
        let state = RuleState {
            armed: row.armed.unwrap_or(true),
            last_triggered_at: row.last_triggered_at,
        };
        match evaluate(&rule, &state, value, now) {
            Decision::Trigger => {
                sqlx::query(
                    "INSERT INTO alert_rule_state (rule_id, armed, last_triggered_at, last_value)
                     VALUES ($1, false, $2, $3)
                     ON CONFLICT (rule_id) DO UPDATE
                       SET armed = false, last_triggered_at = $2, last_value = $3")
                    .bind(row.id).bind(now).bind(value)
                    .execute(pool).await?;
                triggered.push(TriggeredRule {
                    rule_id: row.id,
                    source: row.source,
                    pair: row.pair,
                    price_type: row.price_type,
                    direction: row.direction,
                    threshold: row.threshold,
                    value,
                });
            }
            Decision::Rearm if !state.armed => {
                sqlx::query(
                    "INSERT INTO alert_rule_state (rule_id, armed) VALUES ($1, true)
                     ON CONFLICT (rule_id) DO UPDATE SET armed = true")
                    .bind(row.id).execute(pool).await?;
            }
            _ => {}
        }
    }
    Ok(triggered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn rule(direction: Direction) -> Rule {
        Rule { id: 1, direction, threshold: Decimal::new(730, 2), cooldown_minutes: 60 }
    }

    fn armed() -> RuleState {
        RuleState { armed: true, last_triggered_at: None }
    }

    fn t0() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 10, 0, 0).unwrap()
    }

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn first_breach_triggers() {
        let r = rule(Direction::Above);
        assert_eq!(evaluate(&r, &armed(), d("7.31"), t0()), Decision::Trigger);
    }

    #[test]
    fn below_direction_triggers_on_low() {
        let r = rule(Direction::Below);
        assert_eq!(evaluate(&r, &armed(), d("7.10"), t0()), Decision::Trigger);
        assert_eq!(evaluate(&r, &armed(), d("7.31"), t0()), Decision::Rearm);
    }

    #[test]
    fn still_breached_after_trigger_stays_silent() {
        let r = rule(Direction::Above);
        let st = RuleState { armed: false, last_triggered_at: Some(t0()) };
        assert_eq!(evaluate(&r, &st, d("7.40"), t0() + Duration::hours(2)), Decision::Silent);
    }

    #[test]
    fn recovery_arms_and_rebreach_within_cooldown_is_silent() {
        let r = rule(Direction::Above);
        let triggered = RuleState { armed: false, last_triggered_at: Some(t0()) };
        // 恢复正常 → 再武装
        assert_eq!(evaluate(&r, &triggered, d("7.20"), t0() + Duration::minutes(30)), Decision::Rearm);
        // 冷却内再越限 → 不推（防抖）
        let rearmed = RuleState { armed: true, last_triggered_at: Some(t0()) };
        assert_eq!(evaluate(&r, &rearmed, d("7.31"), t0() + Duration::minutes(30)), Decision::Silent);
        // 冷却过后再越限 → 推
        assert_eq!(evaluate(&r, &rearmed, d("7.31"), t0() + Duration::minutes(61)), Decision::Trigger);
    }
}
