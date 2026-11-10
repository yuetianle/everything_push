pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod rates;
pub mod sources;

use std::sync::Arc;

use config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: Arc<AppConfig>,
    pub auth: crate::auth::AuthState,
}

// axum 未启用 macros feature（无 FromRef derive），手工实现以支持
// Router<AppState> 上挂载提取 State<AuthState> 的 auth handler。
impl axum::extract::FromRef<AppState> for auth::AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}
