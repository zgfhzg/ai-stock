use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    error::{ApiError, ApiResult},
    kis::{self, KisApiResponse},
    risk_settings,
    state::AppState,
};

#[derive(Deserialize, Serialize)]
pub struct OrderRequest {
    pub side: String,
    pub symbol: String,
    pub quantity: u32,
    pub price: u64,
}

#[derive(Serialize)]
pub struct OrderResponse {
    pub accepted: bool,
    pub mode: String,
    pub side: String,
    pub symbol: String,
    pub quantity: u32,
    pub price: u64,
    pub order_amount_krw: u64,
    pub kis: KisApiResponse,
}

#[derive(Serialize)]
struct OrderLogEntry<'a> {
    timestamp_unix: u64,
    request: &'a OrderRequest,
    response: &'a OrderResponse,
}

pub async fn place(state: &AppState, request: OrderRequest) -> ApiResult<OrderResponse> {
    let normalized = normalize_request(request)?;
    validate_live_trading_guard(state)?;
    validate_risk(state, &normalized)?;

    let kis = kis::place_cash_order(
        state,
        &normalized.side,
        &normalized.symbol,
        normalized.quantity,
        normalized.price,
    )
    .await?;

    let response = OrderResponse {
        accepted: kis.rt_cd.as_deref() == Some("0"),
        mode: state.config.trading_mode.clone(),
        side: normalized.side.clone(),
        symbol: normalized.symbol.clone(),
        quantity: normalized.quantity,
        price: normalized.price,
        order_amount_krw: order_amount(&normalized),
        kis,
    };

    append_order_log(&state.config.order_log_path, &normalized, &response)?;
    Ok(response)
}

pub fn list_logs(state: &AppState) -> ApiResult<Vec<Value>> {
    let path = &state.config.order_log_path;
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("order_log_read_failed", error))?;

    let mut rows = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    rows.reverse();
    rows.truncate(50);
    Ok(rows)
}

fn normalize_request(request: OrderRequest) -> ApiResult<OrderRequest> {
    let side = request.side.trim().to_ascii_lowercase();
    if side != "buy" && side != "sell" {
        return validation_error("invalid_order_side", "Order side must be buy or sell.");
    }

    let symbol = request.symbol.trim().to_string();
    if !(symbol.len() == 6 && symbol.chars().all(|char| char.is_ascii_digit())) {
        return validation_error(
            "invalid_symbol",
            "Symbol must be a 6 digit Korean stock code.",
        );
    }

    if request.quantity == 0 {
        return validation_error("invalid_quantity", "Quantity must be greater than zero.");
    }

    if request.price == 0 {
        return validation_error(
            "invalid_price",
            "Only limit orders with a positive price are supported.",
        );
    }

    Ok(OrderRequest {
        side,
        symbol,
        quantity: request.quantity,
        price: request.price,
    })
}

fn validate_live_trading_guard(state: &AppState) -> ApiResult<()> {
    let is_live_mode = matches!(state.config.trading_mode.as_str(), "live" | "real");
    if is_live_mode && !state.config.live_trading_enabled {
        return validation_error(
            "live_trading_disabled",
            "Live trading is disabled. Set ENABLE_LIVE_TRADING=true only after explicit approval.",
        );
    }

    Ok(())
}

fn validate_risk(state: &AppState, request: &OrderRequest) -> ApiResult<()> {
    let amount = order_amount(request);
    let settings = risk_settings::get(state)?;
    if amount > settings.max_order_amount_krw {
        return validation_error(
            "max_order_amount_exceeded",
            "Order amount exceeds MAX_ORDER_AMOUNT_KRW.",
        );
    }

    if today_order_count(state)? >= settings.daily_max_order_count {
        return validation_error(
            "daily_order_count_exceeded",
            "Daily order count limit has been reached.",
        );
    }

    Ok(())
}

fn order_amount(request: &OrderRequest) -> u64 {
    u64::from(request.quantity) * request.price
}

fn today_order_count(state: &AppState) -> ApiResult<u32> {
    let path = &state.config.order_log_path;
    if !Path::new(path).exists() {
        return Ok(0);
    }

    let start = start_of_utc_day(unix_now());
    let content =
        fs::read_to_string(path).map_err(|error| file_error("order_log_read_failed", error))?;
    let count = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| {
            entry
                .get("timestamp_unix")
                .and_then(Value::as_u64)
                .is_some_and(|timestamp| timestamp >= start)
                && entry
                    .get("response")
                    .and_then(|response| response.get("accepted"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .count();

    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

fn start_of_utc_day(timestamp: u64) -> u64 {
    timestamp - (timestamp % 86_400)
}

fn append_order_log(path: &str, request: &OrderRequest, response: &OrderResponse) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("order_log_dir_failed", error))?;
    }

    let entry = OrderLogEntry {
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        request,
        response,
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| file_error("order_log_serialize_failed", error))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| file_error("order_log_open_failed", error))?;
    writeln!(file, "{line}").map_err(|error| file_error("order_log_write_failed", error))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
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
