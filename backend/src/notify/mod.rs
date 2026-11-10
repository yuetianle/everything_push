pub mod api;
pub mod telegram;
pub mod wecom;

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

#[async_trait]
pub trait Notifier: Send + Sync {
    fn channel(&self) -> &'static str;
    async fn send(&self, title: &str, body: &str) -> Result<(), String>;
}

pub async fn dispatch(pool: &PgPool, notifiers: &[Arc<dyn Notifier>], title: &str, body: &str) {
    dispatch_with_backoff(pool, notifiers, title, body, Duration::from_secs(1)).await;
}

pub async fn dispatch_with_backoff(
    pool: &PgPool, notifiers: &[Arc<dyn Notifier>], title: &str, body: &str, base: Duration,
) {
    for n in notifiers {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO notifications (channel, title, body, status) VALUES ($1,$2,$3,'pending') RETURNING id")
            .bind(n.channel()).bind(title).bind(body)
            .fetch_one(pool).await.unwrap_or(0);
        let (status, retries, err) = try_send(n, title, body, base).await;
        let _ = sqlx::query("UPDATE notifications SET status=$2, retries=$3, error=$4 WHERE id=$1")
            .bind(id).bind(status).bind(retries as i32).bind(err)
            .execute(pool).await;
    }
}

async fn try_send(
    n: &Arc<dyn Notifier>, title: &str, body: &str, base: Duration,
) -> (&'static str, u32, Option<String>) {
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match n.send(title, body).await {
            Ok(()) => return ("sent", attempts, None),
            Err(e) if attempts >= 3 => return ("failed", attempts, Some(e)),
            Err(e) => {
                tracing::warn!("{} 推送失败(第{attempts}次): {e}", n.channel());
                tokio::time::sleep(base * 2u32.pow(attempts - 1)).await;
            }
        }
    }
}
