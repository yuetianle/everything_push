use super::Notifier;
use async_trait::async_trait;

pub struct TelegramNotifier {
    pub bot_token: String,
    pub chat_id: String,
    pub api_base: String,
    pub client: reqwest::Client,
}

pub fn build_client(proxy: Option<&str>) -> reqwest::Client {
    let mut b = reqwest::Client::builder();
    if let Some(p) = proxy {
        b = b.proxy(reqwest::Proxy::all(p).expect("非法代理地址"));
    }
    b.build().expect("reqwest client 构建失败")
}

#[async_trait]
impl Notifier for TelegramNotifier {
    fn channel(&self) -> &'static str { "telegram" }

    async fn send(&self, title: &str, body: &str) -> Result<(), String> {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.bot_token);
        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": format!("{title}\n{body}"),
        });
        let resp: serde_json::Value = self.client.post(&url)
            .json(&payload).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        if resp["ok"].as_bool() == Some(true) {
            Ok(())
        } else {
            Err(format!("telegram: {}", resp["description"]))
        }
    }
}
