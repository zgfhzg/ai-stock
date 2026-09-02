use serde::Serialize;
use std::{collections::HashMap, env, fs, path::PathBuf};

#[derive(Clone, Serialize)]
pub struct AppConfig {
    pub env: String,
    pub trading_mode: String,
    pub live_trading_enabled: bool,
    pub strategy_url: String,
    pub kis_app_key: String,
    pub kis_app_secret: String,
    pub kis_account_no: String,
    pub kis_account_product_code: String,
    pub kis_base_url: String,
    pub api_port: u16,
    pub watchlist_path: String,
    pub stock_catalog_path: String,
    pub order_log_path: String,
    pub max_order_amount_krw: u64,
    pub max_position_ratio: f64,
    pub daily_max_loss_ratio: f64,
    pub daily_max_order_count: u32,
}

impl AppConfig {
    pub fn load() -> Self {
        Self {
            env: read_env("APP_ENV", "local"),
            trading_mode: read_env("TRADING_MODE", "paper"),
            live_trading_enabled: read_env("ENABLE_LIVE_TRADING", "false") == "true",
            strategy_url: read_env("STRATEGY_URL", "http://localhost:8090"),
            kis_app_key: read_env("KIS_APP_KEY", ""),
            kis_app_secret: read_env("KIS_APP_SECRET", ""),
            kis_account_no: read_env("KIS_ACCOUNT_NO", ""),
            kis_account_product_code: read_env("KIS_ACCOUNT_PRODUCT_CODE", "01"),
            kis_base_url: read_env(
                "KIS_BASE_URL",
                "https://openapivts.koreainvestment.com:29443",
            ),
            api_port: read_env("API_PORT", "8080").parse().unwrap_or(8080),
            watchlist_path: read_env("WATCHLIST_PATH", "../../data/watchlist.json"),
            stock_catalog_path: read_env("STOCK_CATALOG_PATH", "../../data/stocks.json"),
            order_log_path: read_env("ORDER_LOG_PATH", "../../data/orders.jsonl"),
            max_order_amount_krw: read_env("MAX_ORDER_AMOUNT_KRW", "100000")
                .parse()
                .unwrap_or(100000),
            max_position_ratio: read_env("MAX_POSITION_RATIO", "0.2").parse().unwrap_or(0.2),
            daily_max_loss_ratio: read_env("DAILY_MAX_LOSS_RATIO", "0.03")
                .parse()
                .unwrap_or(0.03),
            daily_max_order_count: read_env("DAILY_MAX_ORDER_COUNT", "20")
                .parse()
                .unwrap_or(20),
        }
    }
}

pub fn load_dotenv() {
    let Some(path) = find_dotenv() else {
        return;
    };
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    for (key, value) in parse_dotenv(&content) {
        if env::var_os(&key).is_none() {
            env::set_var(key, value);
        }
    }
}

fn read_env(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}

fn find_dotenv() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;

    loop {
        let path = dir.join(".env");
        if path.is_file() {
            return Some(path);
        }

        if !dir.pop() {
            return None;
        }
    }
}

fn parse_dotenv(content: &str) -> HashMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }

            let (key, value) = trimmed.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}
