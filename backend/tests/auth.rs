use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use fintrack::auth::*;
use tower::ServiceExt;

fn state() -> AuthState {
    AuthState { password_hash: std::sync::Arc::new("fakehash".into()) }
}

#[test]
fn sign_and_verify_roundtrip() {
    let tok = sign_token("secret", 2000);
    assert!(verify_token("secret", &tok, 1999));
}

#[test]
fn verify_rejects_expired_tampered_wrong_secret() {
    let tok = sign_token("secret", 2000);
    assert!(!verify_token("secret", &tok, 2001), "过期");
    assert!(!verify_token("secret", "2000.deadbeef", 1999), "签名篡改");
    assert!(!verify_token("other", &tok, 1999), "密钥不同");
    assert!(!verify_token("secret", "garbage", 1999));
}

#[tokio::test]
async fn login_wrong_password_401_and_middleware_blocks_without_cookie() {
    let app: Router = auth_routes().with_state(state());
    let resp = app.clone()
        .oneshot(Request::post("/api/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"password":"wrong"}"#)).unwrap())
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // fakehash 不是合法 argon2 串 → 正确密码也 401；这里只验证无 cookie 访问受保护路由被拦截
    // auth_mw 的 401 用一个最小路由验证：
    let protected = axum::Router::new()
        .route("/api/x", axum::routing::get(|| async { "ok" }))
        .route_layer(axum::middleware::from_fn_with_state(state(), auth_mw));
    let resp = protected.oneshot(Request::get("/api/x").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
