use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub admin_password: String,
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

/// 空串/全空白串归一为 None。部署编排（docker-compose 的 `${VAR:-}`）在变量
/// 未设置时会注入空字符串，config crate 的 Environment 源默认不去除空值，
/// 反序列化会得到 Some("")——据此构造的"幽灵"通知渠道（空 token 的
/// Telegram 等）会让每次告警都写失败通知记录，非法代理串更会 panic。
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut cfg: Self = config::Config::builder()
            .add_source(config::File::with_name("fintrack").required(false))
            .add_source(config::Environment::default())
            .build()?
            .try_deserialize()?;
        cfg.normalize();
        Ok(cfg)
    }

    /// 把所有可选字符串字段的空值归一为 None（见 `non_empty`）。
    fn normalize(&mut self) {
        self.wecom_webhook_url = non_empty(self.wecom_webhook_url.take());
        self.telegram_bot_token = non_empty(self.telegram_bot_token.take());
        self.telegram_chat_id = non_empty(self.telegram_chat_id.take());
        self.telegram_proxy = non_empty(self.telegram_proxy.take());
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

    #[test]
    fn non_empty_filters_blank_strings() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(Some("   ".into())), None);
        // 非空值原样保留（不去首尾空白，仅判空白）
        assert_eq!(non_empty(Some(" http://p:1080 ".into())), Some(" http://p:1080 ".into()));
    }

    /// 部署编排会把未设置的 `${VAR:-}` 注入为空串；走与 load 相同的
    /// 反序列化 + normalize 链路后，可选字段必须是 None 而非 Some("")。
    #[test]
    fn empty_optional_strings_normalize_to_none() {
        let toml = concat!(
            r#"database_url = "postgres://fx:fx@localhost/fx""#, "\n",
            r#"admin_password = "hash""#, "\n",
            r#"wecom_webhook_url = """#, "\n",
            r#"telegram_bot_token = """#, "\n",
            r#"telegram_chat_id = "   ""#, "\n",
            r#"telegram_proxy = """#, "\n",
        );
        let mut cfg: AppConfig = config::Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build().unwrap()
            .try_deserialize().unwrap();
        cfg.normalize();
        assert_eq!(cfg.wecom_webhook_url, None);
        assert_eq!(cfg.telegram_bot_token, None);
        assert_eq!(cfg.telegram_chat_id, None);
        assert_eq!(cfg.telegram_proxy, None);

        // 有值字段不受影响
        let toml_set = concat!(
            r#"database_url = "postgres://x""#, "\n",
            r#"admin_password = "h""#, "\n",
            r#"telegram_bot_token = "T""#, "\n",
            r#"telegram_chat_id = "1""#, "\n",
            r#"telegram_proxy = "socks5://127.0.0.1:1080""#, "\n",
        );
        let mut cfg2: AppConfig = config::Config::builder()
            .add_source(config::File::from_str(toml_set, config::FileFormat::Toml))
            .build().unwrap()
            .try_deserialize().unwrap();
        cfg2.normalize();
        assert_eq!(cfg2.telegram_bot_token.as_deref(), Some("T"));
        assert_eq!(cfg2.telegram_chat_id.as_deref(), Some("1"));
        assert_eq!(cfg2.telegram_proxy.as_deref(), Some("socks5://127.0.0.1:1080"));
        assert_eq!(cfg2.wecom_webhook_url, None);
    }
}
