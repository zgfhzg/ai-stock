use reqwest::Client;
use std::{sync::Arc, time::Instant};
use tokio::sync::RwLock;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub http: Client,
    pub kis_token: Arc<RwLock<Option<CachedToken>>>,
}

#[derive(Clone)]
pub struct CachedToken {
    pub access_token: String,
    pub expires_at: Option<String>,
    pub fetched_at: Instant,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
            http: Client::new(),
            kis_token: Arc::new(RwLock::new(None)),
        }
    }
}
