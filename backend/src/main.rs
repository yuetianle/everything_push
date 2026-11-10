use axum::routing::get;
use axum::Router;
use fintrack::{auth, config, db};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = config::AppConfig::load()?;
    let pool = db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    db::ensure_partitions(&pool).await?;
    _ = &pool; // 暂时压制未用警告（Task 4 起进入 AppState）

    let auth_state = auth::AuthState { password_hash: Arc::new(cfg.admin_password.clone()) };
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(auth::auth_routes().with_state(auth_state.clone()));
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
