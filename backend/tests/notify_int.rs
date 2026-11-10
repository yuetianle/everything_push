use std::sync::Arc;
use fintrack::notify::Notifier;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn wecom_send_ok_and_errcode_check() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/webhook/send"))
        .and(body_partial_json(serde_json::json!({"msgtype": "markdown"})))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"errcode":0,"errmsg":"ok"}"#))
        .mount(&server).await;
    let n = fintrack::notify::wecom::WecomNotifier {
        webhook_url: format!("{}/webhook/send", server.uri()),
        client: reqwest::Client::new(),
    };
    assert!(n.send("标题", "内容").await.is_ok());
}

#[tokio::test]
async fn telegram_send_uses_sendmessage() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/bot123:abc/sendMessage"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true,"result":{"message_id":1}}"#))
        .mount(&server).await;
    let n = fintrack::notify::telegram::TelegramNotifier {
        bot_token: "123:abc".into(), chat_id: "10086".into(),
        api_base: server.uri(), client: reqwest::Client::new(),
    };
    assert!(n.send("标题", "内容").await.is_ok());
}

#[tokio::test]
#[ignore]
async fn dispatch_retries_three_times_then_marks_failed() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM notifications").execute(&pool).await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/webhook/send"))
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&server).await;

    let n = Arc::new(fintrack::notify::wecom::WecomNotifier {
        webhook_url: format!("{}/webhook/send", server.uri()),
        client: reqwest::Client::new(),
    });
    let notifiers: Vec<Arc<dyn fintrack::notify::Notifier>> = vec![n];
    fintrack::notify::dispatch_with_backoff(
        &pool, &notifiers, "t", "b", std::time::Duration::from_millis(1)).await;

    let row: (String, i32) = sqlx::query_as(
        "SELECT status, retries FROM notifications WHERE channel='wecom' ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(row, ("failed".into(), 3));
}
