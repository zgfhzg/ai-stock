use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::{
    config::AppConfig,
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct NewsAnalysisSettings {
    pub freshness_days: u64,
    pub daily_article_limit: u64,
    pub daily_cost_limit_usd: f64,
    pub batch_size: usize,
    pub worker_interval_minutes: u64,
    pub retry_base_minutes: u64,
    pub max_attempts: u32,
}

impl NewsAnalysisSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            freshness_days: config.news_analysis_freshness_days,
            daily_article_limit: config.news_analysis_daily_limit,
            daily_cost_limit_usd: config.news_analysis_daily_cost_limit_usd,
            batch_size: config.news_analysis_batch_size,
            worker_interval_minutes: config.news_analysis_worker_interval_minutes,
            retry_base_minutes: config.news_analysis_retry_base_minutes,
            max_attempts: config.news_analysis_max_attempts,
        }
    }
}

pub fn get(state: &AppState) -> ApiResult<NewsAnalysisSettings> {
    read_settings(&state.config.news_analysis_settings_path, &state.config)
}

pub fn save(state: &AppState, settings: NewsAnalysisSettings) -> ApiResult<NewsAnalysisSettings> {
    validate(&settings)?;
    write_settings(&state.config.news_analysis_settings_path, &settings)?;
    Ok(settings)
}

fn read_settings(path: &str, config: &AppConfig) -> ApiResult<NewsAnalysisSettings> {
    if !Path::new(path).exists() {
        return Ok(NewsAnalysisSettings::from_config(config));
    }

    let content = fs::read_to_string(path)
        .map_err(|error| file_error("news_analysis_settings_read_failed", error))?;
    let settings = serde_json::from_str(&content)
        .map_err(|error| file_error("news_analysis_settings_parse_failed", error))?;
    validate(&settings)?;
    Ok(settings)
}

fn write_settings(path: &str, settings: &NewsAnalysisSettings) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("news_analysis_settings_dir_failed", error))?;
    }
    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| file_error("news_analysis_settings_serialize_failed", error))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| file_error("news_analysis_settings_write_failed", error))
}

fn validate(settings: &NewsAnalysisSettings) -> ApiResult<()> {
    let valid = (1..=30).contains(&settings.freshness_days)
        && (1..=5_000).contains(&settings.daily_article_limit)
        && settings.daily_cost_limit_usd.is_finite()
        && (0.001..=100.0).contains(&settings.daily_cost_limit_usd)
        && (1..=20).contains(&settings.batch_size)
        && (1..=1_440).contains(&settings.worker_interval_minutes)
        && (1..=1_440).contains(&settings.retry_base_minutes)
        && (1..=10).contains(&settings.max_attempts);
    if valid {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "invalid_news_analysis_settings".to_string(),
                message: "News analysis settings are outside the allowed range.".to_string(),
            }),
        ))
    }
}

fn file_error(code: &str, error: impl std::fmt::Display) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code: code.to_string(),
            message: error.to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommended_settings_are_valid() {
        let mut config = AppConfig::load();
        config.news_analysis_freshness_days = 7;
        config.news_analysis_daily_limit = 200;
        config.news_analysis_daily_cost_limit_usd = 0.02;
        config.news_analysis_batch_size = 10;
        config.news_analysis_worker_interval_minutes = 5;
        config.news_analysis_retry_base_minutes = 15;
        config.news_analysis_max_attempts = 3;
        assert!(validate(&NewsAnalysisSettings::from_config(&config)).is_ok());
    }
}
