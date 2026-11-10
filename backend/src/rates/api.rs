use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::alerts::engine;
use crate::error::{ApiResult, AppError};
use crate::notify;
use crate::AppState;
use super::{store, RatePoint};

#[derive(Deserialize)]
pub struct ManualBody {
    pair: String,
    fx_bid: Option<Decimal>,
    fx_ask: Option<Decimal>,
    mid: Option<Decimal>,
    note: Option<String>,
}

async fn manual_entry(State(st): State<AppState>, Json(b): Json<ManualBody>) -> ApiResult<Json<serde_json::Value>> {
    if !st.cfg.currency_pairs.iter().any(|p| p == &b.pair) {
        return Err(AppError(StatusCode::BAD_REQUEST, format!("不支持的货币对: {}", b.pair)));
    }
    if b.fx_bid.is_none() && b.fx_ask.is_none() && b.mid.is_none() {
        return Err(AppError(StatusCode::BAD_REQUEST, "至少填一个价格".into()));
    }
    let pt = RatePoint {
        source: "manual".into(),
        pair: b.pair,
        fx_bid: b.fx_bid,
        fx_ask: b.fx_ask,
        mid: b.mid,
        recorded_at: Utc::now(),
        note: b.note,
    };
    store::insert_many(&st.pool, std::slice::from_ref(&pt)).await?;
    // SPEC §3.2：人工数据仅对显式选择 manual 源的规则评估。
    // evaluate_all 按 source='manual' + pair 过滤，其他源的规则不受影响。
    match engine::evaluate_all(&st.pool, &pt, Utc::now()).await {
        Ok(triggered) => {
            for t in triggered {
                notify::dispatch(&st.pool, &st.notifiers, "汇率告警", &t.message()).await;
            }
        }
        Err(e) => tracing::error!("告警评估失败: {e}"),
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn latest(State(st): State<AppState>) -> ApiResult<Json<Vec<super::LatestRate>>> {
    Ok(Json(store::latest(&st.pool).await?))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub pair: String,
    pub sources: Option<String>, // 逗号分隔，默认全部
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub points: Option<u32>,
}

async fn history(State(st): State<AppState>, Query(q): Query<HistoryQuery>) -> ApiResult<Json<serde_json::Value>> {
    let to = q.to.unwrap_or_else(Utc::now);
    let from = q.from.unwrap_or(to - Duration::hours(24));
    let sources: Vec<String> = q.sources
        .map(|s| s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect())
        .unwrap_or_else(|| vec!["api".into(), "boc".into(), "icbc".into(), "manual".into()]);
    let pts = store::history(&st.pool, &q.pair, &sources, from, to, q.points.unwrap_or(300)).await?;
    Ok(Json(serde_json::json!({ "points": pts })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/rates/latest", get(latest))
        .route("/api/rates/history", get(history))
        .route("/api/rates/manual", post(manual_entry))
}
