use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::error::{ApiResult, AppError};
use crate::AppState;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct RuleRow {
    pub id: i64,
    pub source: String,
    pub pair: String,
    pub price_type: String,
    pub direction: String,
    pub threshold: Decimal,
    pub cooldown_minutes: i32,
    pub enabled: bool,
}

async fn list(State(st): State<AppState>) -> ApiResult<Json<Vec<RuleRow>>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled
         FROM alert_rules ORDER BY id")
        .fetch_all(&st.pool).await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct RuleBody {
    pub source: String,
    pub pair: String,
    pub price_type: String,     // fx_bid | fx_ask | mid
    pub direction: String,      // above | below
    pub threshold: Decimal,
    #[serde(default = "d_cd")]
    pub cooldown_minutes: i32,
    pub enabled: bool,
}

fn d_cd() -> i32 { 60 }

fn validate(b: &RuleBody) -> Result<(), AppError> {
    let ok_pt = matches!(b.price_type.as_str(), "fx_bid" | "fx_ask" | "mid");
    let ok_dir = matches!(b.direction.as_str(), "above" | "below");
    let ok_src = matches!(b.source.as_str(), "api" | "boc" | "icbc" | "manual");
    if ok_pt && ok_dir && ok_src {
        Ok(())
    } else {
        Err(AppError(StatusCode::BAD_REQUEST, "price_type/direction/source 非法".into()))
    }
}

async fn create(State(st): State<AppState>, Json(b): Json<RuleBody>) -> ApiResult<Json<RuleRow>> {
    validate(&b)?;
    let row = sqlx::query_as::<_, RuleRow>(
        "INSERT INTO alert_rules (source, pair, price_type, direction, threshold, cooldown_minutes, enabled)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled")
        .bind(&b.source).bind(&b.pair).bind(&b.price_type).bind(&b.direction)
        .bind(b.threshold).bind(b.cooldown_minutes).bind(b.enabled)
        .fetch_one(&st.pool).await?;
    Ok(Json(row))
}

async fn update(State(st): State<AppState>, Path(id): Path<i64>, Json(b): Json<RuleBody>) -> ApiResult<Json<RuleRow>> {
    validate(&b)?;
    let row = sqlx::query_as::<_, RuleRow>(
        "UPDATE alert_rules SET source=$2, pair=$3, price_type=$4, direction=$5,
                threshold=$6, cooldown_minutes=$7, enabled=$8
         WHERE id=$1
         RETURNING id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled")
        .bind(id).bind(&b.source).bind(&b.pair).bind(&b.price_type).bind(&b.direction)
        .bind(b.threshold).bind(b.cooldown_minutes).bind(b.enabled)
        .fetch_optional(&st.pool).await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("规则 {id} 不存在")))?;
    Ok(Json(row))
}

async fn delete(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    let r = sqlx::query("DELETE FROM alert_rules WHERE id=$1").bind(id).execute(&st.pool).await?;
    if r.rows_affected() == 0 {
        return Err(AppError(StatusCode::NOT_FOUND, format!("规则 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/alert-rules", get(list).post(create))
        // axum 0.8 路径参数语法为 {id}（:id 会在注册时 panic）
        .route("/api/alert-rules/{id}", put(update).delete(delete))
}
