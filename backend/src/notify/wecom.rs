use super::Notifier;
use async_trait::async_trait;

pub struct WecomNotifier {
    pub webhook_url: String,
    pub client: reqwest::Client,
}

#[async_trait]
impl Notifier for WecomNotifier {
    fn channel(&self) -> &'static str { "wecom" }

    async fn send(&self, title: &str, body: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "msgtype": "markdown",
            "markdown": { "content": format!("**{title}**\n{body}") },
        });
        let resp: serde_json::Value = self.client.post(&self.webhook_url)
            .json(&payload).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        if resp["errcode"].as_i64() == Some(0) {
            Ok(())
        } else {
            Err(format!("wecom errcode: {}", resp["errcode"]))
        }
    }
}
