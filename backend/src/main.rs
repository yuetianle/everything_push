use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

use fintrack::notify::{telegram::TelegramNotifier, wecom::WecomNotifier, Notifier};
use fintrack::sources::health::HealthRegistry;
use fintrack::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = Arc::new(fintrack::config::AppConfig::load()?);
    let pool = fintrack::db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    fintrack::db::ensure_partitions(&pool).await?;

    let auth_state = fintrack::auth::AuthState {
        password_hash: Arc::new(cfg.admin_password.clone()),
    };

    // 通知渠道：配置了哪个就启用哪个
    let mut notifiers: Vec<Arc<dyn Notifier>> = vec![];
    if let Some(url) = &cfg.wecom_webhook_url {
        notifiers.push(Arc::new(WecomNotifier { webhook_url: url.clone(), client: reqwest::Client::new() }));
    }
    if let (Some(token), Some(chat)) = (&cfg.telegram_bot_token, &cfg.telegram_chat_id) {
        notifiers.push(Arc::new(TelegramNotifier {
            bot_token: token.clone(), chat_id: chat.clone(),
            api_base: "https://api.telegram.org".into(),
            client: fintrack::notify::telegram::build_client(cfg.telegram_proxy.as_deref()),
        }));
    }
    if notifiers.is_empty() {
        tracing::warn!("未配置任何通知渠道（WECOM/TELEGRAM），告警与提醒将只入库不推送");
    }

    let state = AppState {
        pool: pool.clone(),
        cfg: cfg.clone(),
        auth: auth_state.clone(),
        health: Arc::new(HealthRegistry::new()),
        notifiers: Arc::new(notifiers),
    };

    // 分层组装：public 无需 cookie；protected 整层经 auth_mw（route_layer 只影响
    // 此前已注册的路由，因此先注册全部受保护路由再套层，最后并入 public）。
    let public = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/auth/login", post(fintrack::auth::login_handler));

    let protected = Router::new()
        .route("/api/auth/logout", post(fintrack::auth::logout_handler))
        .route("/api/auth/me", get(fintrack::auth::me_handler))
        .merge(fintrack::rates::api::routes())
        .merge(fintrack::alerts::api::routes())
        .merge(fintrack::notify::api::routes())
        .merge(fintrack::deposits::api::routes())
        .route("/api/sources/status", get(sources_status))
        .route_layer(axum::middleware::from_fn_with_state(auth_state, fintrack::auth::auth_mw));

    let app = public.merge(protected).with_state(state.clone());

    // 后台任务：银行抓取 / 基准增量 / 每日 09:05
    fintrack::scheduler::spawn_all(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn sources_status(
    axum::extract::State(st): axum::extract::State<fintrack::AppState>,
) -> axum::Json<Vec<fintrack::sources::health::SourceSnapshot>> {
    axum::Json(st.health.snapshots())
}
