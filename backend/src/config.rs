use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub admin_password_hash: String,
    pub wecom_webhook_url: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub telegram_proxy: Option<String>,
    #[serde(default = "d_interval")]
    pub scrape_interval_secs: u64,
    #[serde(default = "d_pairs")]
    pub currency_pairs: Vec<String>,
}

fn d_interval() -> u64 { 180 }
fn d_pairs() -> Vec<String> {
    ["USD", "EUR", "JPY", "HKD", "GBP", "AUD"]
        .iter().map(|c| format!("{c}/CNY")).collect()
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name("fintrack").required(false))
            .add_source(config::Environment::default())
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_pairs_are_six_cny_pairs() {
        assert_eq!(d_pairs(), vec![
            "USD/CNY", "EUR/CNY", "JPY/CNY", "HKD/CNY", "GBP/CNY", "AUD/CNY"]);
    }
}
