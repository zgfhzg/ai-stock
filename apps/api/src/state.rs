use reqwest::Client;
use std::{sync::Arc, time::Instant};
use tokio::sync::{Mutex, RwLock};

use crate::config::AppConfig;
use crate::rule_monitor::RuleMonitorRuntime;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub http: Client,
    pub kis_token: Arc<RwLock<Option<CachedToken>>>,
    pub rule_monitor: Arc<Mutex<RuleMonitorRuntime>>,
}

#[derive(Clone)]
pub struct CachedToken {
    pub access_token: String,
    pub expires_at: Option<String>,
    pub fetched_at: Instant,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let monitor_interval_seconds = config.auto_rule_monitor_interval_seconds;
        Self {
            config: Arc::new(config),
            http: Client::new(),
            kis_token: Arc::new(RwLock::new(None)),
            rule_monitor: Arc::new(Mutex::new(RuleMonitorRuntime::new(
                monitor_interval_seconds,
            ))),
        }
    }
}
