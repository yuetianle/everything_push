use axum::routing::{get, post};
use axum::Router;
use fintrack::{auth, config, db, AppState};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = Arc::new(config::AppConfig::load()?);
    let pool = db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    db::ensure_partitions(&pool).await?;

    let auth_state = auth::AuthState { password_hash: Arc::new(cfg.admin_password.clone()) };
    let state = AppState { pool: pool.clone(), cfg: cfg.clone(), auth: auth_state.clone() };

    // 分层组装：public 无需 cookie；protected 整层经 auth_mw（route_layer 只影响
    // 此前已注册的路由，因此先 merge 全部受保护路由再套层）。
    let public = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/auth/login", post(auth::login_handler));

    let protected = Router::new()
        .route("/api/auth/logout", post(auth::logout_handler))
        .route("/api/auth/me", get(auth::me_handler))
        .merge(fintrack::rates::api::routes())
        .merge(fintrack::alerts::api::routes())
        .merge(fintrack::deposits::api::routes())
        .merge(fintrack::notify::api::routes());

    let app = public
        .merge(protected.route_layer(axum::middleware::from_fn_with_state(auth_state, auth::auth_mw)))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
