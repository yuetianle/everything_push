use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub channel: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub page: i64,
    #[serde(default = "d_size")]
    pub page_size: i64,
}

fn d_size() -> i64 { 20 }

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct NotificationRow {
    pub id: i64,
    pub channel: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub retries: i32,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn list(State(st): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<serde_json::Value>> {
    let page = q.page.max(1);
    let size = q.page_size.clamp(1, 100);
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT id, channel, title, body, status, retries, error, created_at
         FROM notifications
         WHERE ($1::text IS NULL OR channel = $1)
           AND ($2::text IS NULL OR status = $2)
         ORDER BY id DESC LIMIT $3 OFFSET $4")
        .bind(&q.channel).bind(&q.status).bind(size).bind((page - 1) * size)
        .fetch_all(&st.pool).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE ($1::text IS NULL OR channel = $1) AND ($2::text IS NULL OR status = $2)")
        .bind(&q.channel).bind(&q.status)
        .fetch_one(&st.pool).await?;
    Ok(Json(serde_json::json!({ "rows": rows, "total": total })))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/notifications", get(list))
}
