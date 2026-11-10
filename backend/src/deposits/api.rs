use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::error::{ApiResult, AppError};
use crate::AppState;
use super::NewDeposit;
use crate::deposits as dep;

fn validate(b: &NewDeposit) -> Result<(), AppError> {
    if b.amount <= rust_decimal::Decimal::ZERO || b.term_months <= 0 {
        return Err(AppError(StatusCode::BAD_REQUEST, "金额与期限必须为正".into()));
    }
    if b.currency.len() != 3 || !b.currency.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError(StatusCode::BAD_REQUEST, "币种须为 3 位大写字母".into()));
    }
    Ok(())
}

async fn list(State(st): State<AppState>) -> ApiResult<Json<Vec<dep::Deposit>>> {
    Ok(Json(dep::list(&st.pool).await?))
}

async fn create(State(st): State<AppState>, Json(b): Json<NewDeposit>) -> ApiResult<Json<serde_json::Value>> {
    validate(&b)?;
    let id = dep::create(&st.pool, &b).await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn update(State(st): State<AppState>, Path(id): Path<i64>, Json(b): Json<NewDeposit>) -> ApiResult<Json<serde_json::Value>> {
    validate(&b)?;
    if !dep::update(&st.pool, id, &b).await? {
        return Err(AppError(StatusCode::NOT_FOUND, format!("存款 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn delete(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    if !dep::delete(&st.pool, id).await? {
        return Err(AppError(StatusCode::NOT_FOUND, format!("存款 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn summary(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let s = dep::summary(&st.pool).await?;
    let due = dep::due_in_30d(&st.pool).await?;
    Ok(Json(serde_json::json!({ "currencies": s, "due_30d": due })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/deposits", get(list).post(create))
        // axum 0.8 路径参数语法为 {id}（:id 会在注册时 panic）
        .route("/api/deposits/{id}", axum::routing::put(update).delete(delete))
        .route("/api/deposits/summary", get(summary))
}
