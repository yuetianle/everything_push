use argon2::PasswordVerifier;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::error::{ApiResult, AppError};

pub const COOKIE_NAME: &str = "fx_session";
pub const SESSION_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AuthState {
    pub password_hash: Arc<String>,
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    match argon2::PasswordHash::new(hash) {
        Ok(parsed) => {
            argon2::Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
        }
        Err(_) => false,
    }
}

fn hmac_hex(secret: &str, msg: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn sign_token(secret: &str, expires_at_unix: i64) -> String {
    let exp = expires_at_unix.to_string();
    format!("{exp}.{}", hmac_hex(secret, &exp))
}

pub fn verify_token(secret: &str, token: &str, now_unix: i64) -> bool {
    let Some((exp, sig)) = token.split_once('.') else { return false };
    let Ok(exp) = exp.parse::<i64>() else { return false };
    if now_unix > exp {
        return false;
    }
    let expect = hmac_hex(secret, exp.to_string().as_str());
    expect.as_bytes().ct_eq(sig.as_bytes()).into()
}

fn cookie_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == COOKIE_NAME).then(|| v.to_string())
    })
}

pub async fn auth_mw(State(st): State<AuthState>, req: Request, next: Next) -> Response {
    let now = chrono::Utc::now().timestamp();
    let ok = cookie_from_headers(req.headers())
        .map(|tok| verify_token(&st.password_hash, &tok, now))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        AppError(StatusCode::UNAUTHORIZED, "未登录".into()).into_response()
    }
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(State(st): State<AuthState>, Json(body): Json<LoginBody>) -> ApiResult<Response> {
    if !verify_password(&st.password_hash, &body.password) {
        return Err(AppError(StatusCode::UNAUTHORIZED, "密码错误".into()));
    }
    let now = chrono::Utc::now();
    let token = sign_token(&st.password_hash, now.timestamp() + SESSION_DAYS * 86400);
    let cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 86400
    );
    let mut resp = (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&cookie).unwrap(),
    );
    Ok(resp)
}

async fn logout() -> Response {
    let mut resp = (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&format!("{COOKIE_NAME}=; Path=/; Max-Age=0")).unwrap(),
    );
    resp
}

async fn me() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"ok": true}))
}

pub fn auth_routes() -> Router<AuthState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}
