use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::{
    config::AppConfig,
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct RiskSettings {
    pub max_order_amount_krw: u64,
    pub max_daily_auto_order_amount_krw_per_symbol: u64,
    pub max_position_ratio: f64,
    pub daily_max_loss_ratio: f64,
    pub daily_max_order_count: u32,
    pub max_crypto_order_amount_usdt: f64,
}

#[derive(Deserialize)]
pub struct RiskSettingsInput {
    pub max_order_amount_krw: u64,
    pub max_daily_auto_order_amount_krw_per_symbol: u64,
    pub max_position_ratio: f64,
    pub daily_max_loss_ratio: f64,
    pub daily_max_order_count: u32,
    pub max_crypto_order_amount_usdt: f64,
}

impl RiskSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            max_order_amount_krw: config.max_order_amount_krw,
            max_daily_auto_order_amount_krw_per_symbol: config
                .max_daily_auto_order_amount_krw_per_symbol,
            max_position_ratio: config.max_position_ratio,
            daily_max_loss_ratio: config.daily_max_loss_ratio,
            daily_max_order_count: config.daily_max_order_count,
            max_crypto_order_amount_usdt: config.max_crypto_order_amount_usdt,
        }
    }
}

pub fn get(state: &AppState) -> ApiResult<RiskSettings> {
    read_settings(&state.config.risk_settings_path, &state.config)
}

pub fn save(state: &AppState, input: RiskSettingsInput) -> ApiResult<RiskSettings> {
    validate(&input)?;

    let settings = RiskSettings {
        max_order_amount_krw: input.max_order_amount_krw,
        max_daily_auto_order_amount_krw_per_symbol: input
            .max_daily_auto_order_amount_krw_per_symbol,
        max_position_ratio: input.max_position_ratio,
        daily_max_loss_ratio: input.daily_max_loss_ratio,
        daily_max_order_count: input.daily_max_order_count,
        max_crypto_order_amount_usdt: input.max_crypto_order_amount_usdt,
    };

    write_settings(&state.config.risk_settings_path, &settings)?;
    Ok(settings)
}

fn read_settings(path: &str, config: &AppConfig) -> ApiResult<RiskSettings> {
    if !Path::new(path).exists() {
        return Ok(RiskSettings::from_config(config));
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("risk_read_failed", error))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|error| file_error("risk_parse_failed", error))?;
    let defaults = RiskSettings::from_config(config);

    Ok(RiskSettings {
        max_order_amount_krw: value
            .get("max_order_amount_krw")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(defaults.max_order_amount_krw),
        max_daily_auto_order_amount_krw_per_symbol: value
            .get("max_daily_auto_order_amount_krw_per_symbol")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(defaults.max_daily_auto_order_amount_krw_per_symbol),
        max_position_ratio: value
            .get("max_position_ratio")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(defaults.max_position_ratio),
        daily_max_loss_ratio: value
            .get("daily_max_loss_ratio")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(defaults.daily_max_loss_ratio),
        daily_max_order_count: value
            .get("daily_max_order_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(defaults.daily_max_order_count),
        max_crypto_order_amount_usdt: value
            .get("max_crypto_order_amount_usdt")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(defaults.max_crypto_order_amount_usdt),
    })
}

fn write_settings(path: &str, settings: &RiskSettings) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("risk_dir_failed", error))?;
    }

    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| file_error("risk_serialize_failed", error))?;
    fs::write(path, format!("{content}\n")).map_err(|error| file_error("risk_write_failed", error))
}

fn validate(input: &RiskSettingsInput) -> ApiResult<()> {
    if input.max_order_amount_krw == 0 {
        return validation_error(
            "invalid_max_order_amount",
            "Stock order limit must be positive.",
        );
    }
    if input.max_daily_auto_order_amount_krw_per_symbol == 0 {
        return validation_error(
            "invalid_max_daily_auto_order_amount",
            "Daily auto order amount limit must be positive.",
        );
    }
    if input.max_crypto_order_amount_usdt <= 0.0 || !input.max_crypto_order_amount_usdt.is_finite()
    {
        return validation_error(
            "invalid_max_crypto_order_amount",
            "Crypto order limit must be positive.",
        );
    }
    if input.max_position_ratio <= 0.0
        || input.max_position_ratio > 1.0
        || !input.max_position_ratio.is_finite()
    {
        return validation_error(
            "invalid_max_position_ratio",
            "Position ratio must be between 0 and 1.",
        );
    }
    if input.daily_max_loss_ratio <= 0.0
        || input.daily_max_loss_ratio > 1.0
        || !input.daily_max_loss_ratio.is_finite()
    {
        return validation_error(
            "invalid_daily_max_loss_ratio",
            "Daily loss ratio must be between 0 and 1.",
        );
    }
    if input.daily_max_order_count == 0 {
        return validation_error(
            "invalid_daily_max_order_count",
            "Daily order count must be positive.",
        );
    }

    Ok(())
}

fn validation_error<T>(code: &str, message: &str) -> ApiResult<T> {
    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            code: code.to_string(),
            message: message.to_string(),
        }),
    ))
}

fn file_error(
    code: impl Into<String>,
    error: impl std::fmt::Display,
) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code: code.into(),
            message: error.to_string(),
        }),
    )
}
