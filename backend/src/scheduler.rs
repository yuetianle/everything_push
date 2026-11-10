use crate::alerts::engine;
use crate::notify;
use crate::rates::store;
use crate::sources::health::HealthEvent;
use crate::{sources, AppState};
use chrono::Utc;

async fn ingest_point_eval_alerts(state: &AppState, pts: &[crate::rates::RatePoint]) {
    if let Err(e) = store::insert_many(&state.pool, pts).await {
        tracing::error!("入库失败: {e}");
        return;
    }
    for pt in pts {
        match engine::evaluate_all(&state.pool, pt, Utc::now()).await {
            Ok(triggered) => {
                for t in triggered {
                    notify::dispatch(&state.pool, &state.notifiers, "汇率告警", &t.message()).await;
                }
            }
            Err(e) => tracing::error!("告警评估失败: {e}"),
        }
    }
}

async fn notify_health(state: &AppState, label: &str, ev: HealthEvent) {
    let body = match ev {
        HealthEvent::WentDown => format!("数据源【{label}】失效（连续 3 次抓取失败）"),
        HealthEvent::Recovered => format!("数据源【{label}】已恢复"),
    };
    notify::dispatch(&state.pool, &state.notifiers, "数据源状态", &body).await;
}

/// 泛型闭包注入 fetch 逻辑（F: FnOnce() -> Fut），调用处传 `|| async { ... }`。
async fn run_bank_scraper<F, Fut>(
    state: &AppState,
    name: &'static str,
    label: &str,
    fetch: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<crate::rates::RatePoint>, String>>,
{
    match fetch().await {
        Ok(pts) => {
            if let Some(ev) = state.health.report(name, true) {
                notify_health(state, label, ev).await;
            }
            ingest_point_eval_alerts(state, &pts).await;
        }
        Err(e) => {
            tracing::warn!("{label} 抓取失败: {e}");
            if let Some(ev) = state.health.report(name, false) {
                notify_health(state, label, ev).await;
            }
        }
    }
}

async fn run_franks(state: &AppState) {
    let client = reqwest::Client::new();
    match sources::frankfurter::ingest_api_rates(&state.pool, &client, &state.cfg.currency_pairs).await {
        Ok(n) => {
            tracing::info!("基准汇率增量 {n} 条");
            // api 源的健康事件不推送（与银行源区分），仅记录状态
            let _ = state.health.report("api", true);
            // 基准价（source=api）上的规则评估：取每对最新一条评估
            for pair in &state.cfg.currency_pairs {
                if let Ok(Some(pt)) = store::latest_one(&state.pool, "api", pair).await {
                    if let Ok(triggered) = engine::evaluate_all(&state.pool, &pt, Utc::now()).await {
                        for t in triggered {
                            notify::dispatch(&state.pool, &state.notifiers, "汇率告警", &t.message()).await;
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("基准汇率拉取失败: {e}");
            let _ = state.health.report("api", false);
        }
    }
}

async fn run_daily(state: &AppState) {
    // R9：到期档位按北京日期判定（09:05 北京 = 01:05 UTC，两者日期可能不同）
    let today = Utc::now().with_timezone(&chrono_tz::Asia::Shanghai).date_naive();
    match crate::maturity::scan_and_mark(&state.pool, today).await {
        Ok(notices) => {
            for n in notices {
                notify::dispatch(&state.pool, &state.notifiers, "定期存款", &crate::maturity::notice_text(&n)).await;
            }
        }
        Err(e) => tracing::error!("到期扫描失败: {e}"),
    }
    if let Err(e) = crate::db::ensure_partitions(&state.pool).await {
        tracing::error!("分区维护失败: {e}");
    }
    if let Err(e) = crate::db::drop_old_partitions(&state.pool, 24).await {
        tracing::error!("分区清理失败: {e}");
    }
}

pub fn spawn_all(state: AppState) {
    // ① 银行抓取循环：每轮依次抓中行、工行，然后睡 scrape_interval_secs
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                let s = state.clone();
                run_bank_scraper(&s, "boc", "中行", || async {
                    let html = sources::boc::fetch(&reqwest::Client::new()).await?;
                    sources::boc::parse(&html, Utc::now())
                }).await;
                let s = state.clone();
                run_bank_scraper(&s, "icbc", "工行", || async {
                    let html = sources::icbc::fetch(&reqwest::Client::new()).await?;
                    sources::icbc::parse(&html, Utc::now())
                }).await;
                tokio::time::sleep(std::time::Duration::from_secs(state.cfg.scrape_interval_secs)).await;
            }
        });
    }
    // ② 基准 API 每小时增量 + 启动立即执行一次（回填/快速见效）
    {
        let state = state.clone();
        tokio::spawn(async move {
            run_franks(&state).await;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                run_franks(&state).await;
            }
        });
    }
    // ③ 每日 09:05（北京时间）：到期扫描 + 分区维护/清理
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                let next = crate::maturity::next_0905(Utc::now());
                // 睡到下一个 09:05（next 恒在未来；临界竞态下负时长按 0 处理立即执行）
                let wait = (next - Utc::now()).to_std().unwrap_or_default();
                tokio::time::sleep(wait).await;
                run_daily(&state).await;
            }
        });
    }
}
