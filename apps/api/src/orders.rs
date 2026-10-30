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
    pub purchase_price: Option<f64>,
    pub kis: KisApiResponse,
}

#[derive(Serialize)]
struct OrderLogEntry<'a> {
    timestamp_unix: u64,
    request: &'a OrderRequest,
    response: &'a OrderResponse,
}

pub async fn place(state: &AppState, request: OrderRequest) -> ApiResult<OrderResponse> {
    let normalized = validate_proposed_order(state, request)?;
    validate_live_trading_guard(state)?;

    // Preserve the cost basis before a sell removes the holding from the account.
    let purchase_price = if normalized.side == "sell" {
        match kis::get_balance(state).await {
            Ok(balance) if balance.rt_cd.as_deref() == Some("0") => balance
                .output1
                .as_ref()
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row["pdno"].as_str() == Some(&normalized.symbol))
                })
                .and_then(|row| {
                    row["pchs_avg_pric"]
                        .as_str()
                        .and_then(|v| v.replace(',', "").parse::<f64>().ok())
                        .or_else(|| row["pchs_avg_pric"].as_f64())
                })
                .filter(|v| v.is_finite() && *v > 0.0),
            _ => None,
        }
    } else {
        None
    };

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
        purchase_price,
        kis,
    };

    append_order_log(&state.config.order_log_path, &normalized, &response)?;
    Ok(response)
}

pub fn validate_proposed_order(state: &AppState, request: OrderRequest) -> ApiResult<OrderRequest> {
    let normalized = normalize_request(request)?;
    validate_risk(state, &normalized)?;
    Ok(normalized)
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
    let db = rusqlite::Connection::open_with_flags(
        &state.config.news_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok();
    for row in &mut rows {
        let symbol = row["response"]["symbol"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if let Ok(Some(stock)) = crate::stocks::resolve_news_stock(state, "", &symbol) {
            row["stock_name"] = Value::String(stock.name);
        }
        if let Some(db) = &db {
            let order_no = row["response"]["kis"]["output"]["ODNO"]
                .as_str()
                .or_else(|| row["response"]["kis"]["output"]["odno"].as_str())
                .unwrap_or_default();
            if !order_no.is_empty() && row["response"]["accepted"].as_bool() == Some(true) {
                let raw: Result<String, _> = db.query_row(
                    "SELECT record_json FROM ai_order_events WHERE symbol=?1
                     AND json_extract(record_json, '$.broker_order.order_no')=?2
                     AND json_extract(record_json, '$.broker_order.order_date')=strftime('%Y%m%d', ?3, 'unixepoch', '+9 hours')
                     AND json_extract(record_json, '$.broker_order.side')=?4 ORDER BY id DESC LIMIT 1",
                    rusqlite::params![symbol, order_no, row["timestamp_unix"].as_u64().unwrap_or(0), row["response"]["side"].as_str().unwrap_or_default()],
                    |r| r.get(0));
                if let Ok(raw) = raw {
                    if let Ok(record) = serde_json::from_str::<Value>(&raw) {
                        row["execution"] = record["broker_order"].clone();
                    }
                }
            }
        }
        let basis = row["response"]["purchase_price"].as_f64();
        let amount = row["execution"]["filled_amount_krw"].as_f64();
        let quantity = row["execution"]["filled_quantity"].as_u64().unwrap_or(0);
        if row["response"]["side"].as_str() == Some("sell")
            && row["response"]["accepted"].as_bool() == Some(true)
        {
            if let Some((profit, rate)) = realized_profit(basis, amount, quantity) {
                row["profit_krw"] = serde_json::json!(profit);
                row["profit_percent"] = serde_json::json!(rate);
            }
        }
    }
    Ok(rows)
}

fn realized_profit(basis: Option<f64>, amount: Option<f64>, quantity: u64) -> Option<(f64, f64)> {
    let cost = basis? * quantity as f64;
    let proceeds = amount?;
    if !cost.is_finite() || cost <= 0.0 || !proceeds.is_finite() || proceeds <= 0.0 {
        return None;
    }
    Some((proceeds - cost, (proceeds / cost - 1.0) * 100.0))
}

#[cfg(test)]
mod profit_tests {
    use super::realized_profit;
    #[test]
    fn actual_partial_fill_profit_and_loss() {
        let (profit, rate) = realized_profit(Some(100.0), Some(220.0), 2).unwrap();
        assert_eq!(profit, 20.0);
        assert!((rate - 10.0).abs() < 0.000001);
        assert_eq!(
            realized_profit(Some(100.0), Some(180.0), 2).unwrap().0,
            -20.0
        );
    }
    #[test]
    fn missing_or_unfilled_is_not_zero_profit() {
        assert!(realized_profit(None, Some(200.0), 2).is_none());
        assert!(realized_profit(Some(100.0), None, 2).is_none());
        assert!(realized_profit(Some(100.0), Some(200.0), 0).is_none());
    }
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

pub(crate) fn today_order_count(state: &AppState) -> ApiResult<u32> {
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
