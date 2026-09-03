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
    error::{api_error, ApiError, ApiResult},
    state::AppState,
};

#[derive(Clone, Serialize)]
pub struct CryptoConfigStatus {
    pub exchange: String,
    pub spot_base_url: String,
    pub futures_base_url: String,
    pub api_key_configured: bool,
    pub api_secret_configured: bool,
    pub live_trading_enabled: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct CryptoInstrument {
    pub symbol: String,
    pub name: String,
    pub venue: String,
    pub market_type: String,
    pub quote_asset: String,
    pub max_leverage: Option<u32>,
}

#[derive(Deserialize, Serialize)]
pub struct CryptoOrderRequest {
    pub market_type: String,
    pub side: String,
    pub symbol: String,
    pub quantity: f64,
    pub price: f64,
    pub leverage: Option<u32>,
}

#[derive(Serialize)]
pub struct CryptoOrderResponse {
    pub accepted: bool,
    pub mode: String,
    pub venue: String,
    pub market_type: String,
    pub side: String,
    pub symbol: String,
    pub quantity: f64,
    pub price: f64,
    pub notional_usdt: f64,
    pub leverage: Option<u32>,
    pub status: String,
    pub message: String,
}

#[derive(Serialize)]
struct CryptoOrderLogEntry<'a> {
    timestamp_unix: u64,
    request: &'a CryptoOrderRequest,
    response: &'a CryptoOrderResponse,
}

#[derive(Deserialize)]
struct BinanceTicker {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "priceChange")]
    price_change: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
    #[serde(rename = "highPrice")]
    high_price: String,
    #[serde(rename = "lowPrice")]
    low_price: String,
    #[serde(rename = "volume")]
    volume: String,
    #[serde(rename = "quoteVolume")]
    quote_volume: Option<String>,
    #[serde(rename = "count")]
    count: Option<u64>,
}

#[derive(Serialize)]
pub struct CryptoQuote {
    pub symbol: String,
    pub market_type: String,
    pub venue: String,
    pub last_price: String,
    pub price_change: String,
    pub price_change_percent: String,
    pub high_price: String,
    pub low_price: String,
    pub volume: String,
    pub quote_volume: Option<String>,
    pub trade_count: Option<u64>,
}

pub fn config_status(state: &AppState) -> CryptoConfigStatus {
    CryptoConfigStatus {
        exchange: state.config.crypto_exchange.clone(),
        spot_base_url: state.config.crypto_spot_base_url.clone(),
        futures_base_url: state.config.crypto_futures_base_url.clone(),
        api_key_configured: !state.config.crypto_api_key.is_empty(),
        api_secret_configured: !state.config.crypto_api_secret.is_empty(),
        live_trading_enabled: state.config.crypto_live_trading_enabled,
    }
}

pub fn instruments(market_type: &str) -> ApiResult<Vec<CryptoInstrument>> {
    let market_type = normalize_market_type(market_type)?;
    Ok(default_instruments()
        .into_iter()
        .filter(|instrument| instrument.market_type == market_type)
        .collect())
}

pub async fn quote(state: &AppState, market_type: &str, symbol: &str) -> ApiResult<CryptoQuote> {
    let market_type = normalize_market_type(market_type)?;
    let symbol = normalize_symbol(symbol)?;
    let base_url = match market_type.as_str() {
        "spot" => &state.config.crypto_spot_base_url,
        "futures" => &state.config.crypto_futures_base_url,
        _ => unreachable!(),
    };
    let path = match market_type.as_str() {
        "spot" => "/api/v3/ticker/24hr",
        "futures" => "/fapi/v1/ticker/24hr",
        _ => unreachable!(),
    };
    let url = format!("{base_url}{path}");

    let ticker = state
        .http
        .get(url)
        .query(&[("symbol", symbol.as_str())])
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::BAD_GATEWAY,
                "crypto_quote_request_failed",
                error,
            )
        })?
        .error_for_status()
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "crypto_quote_http_error", error))?
        .json::<BinanceTicker>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "crypto_quote_parse_failed", error))?;

    Ok(CryptoQuote {
        symbol: ticker.symbol,
        market_type,
        venue: state.config.crypto_exchange.clone(),
        last_price: ticker.last_price,
        price_change: ticker.price_change,
        price_change_percent: ticker.price_change_percent,
        high_price: ticker.high_price,
        low_price: ticker.low_price,
        volume: ticker.volume,
        quote_volume: ticker.quote_volume,
        trade_count: ticker.count,
    })
}

pub async fn place_order(
    state: &AppState,
    request: CryptoOrderRequest,
) -> ApiResult<CryptoOrderResponse> {
    let normalized = normalize_order_request(request)?;
    validate_crypto_risk(state, &normalized)?;

    let live_mode = matches!(state.config.trading_mode.as_str(), "live" | "real")
        && state.config.crypto_live_trading_enabled;
    if live_mode {
        return validation_error(
            "crypto_live_order_not_implemented",
            "Crypto live order routing is intentionally not enabled yet. Add signed exchange order support after testnet validation.",
        );
    }

    let response = CryptoOrderResponse {
        accepted: true,
        mode: "paper".to_string(),
        venue: state.config.crypto_exchange.clone(),
        market_type: normalized.market_type.clone(),
        side: normalized.side.clone(),
        symbol: normalized.symbol.clone(),
        quantity: normalized.quantity,
        price: normalized.price,
        notional_usdt: notional(&normalized),
        leverage: normalized.leverage,
        status: "paper_accepted".to_string(),
        message: "Paper crypto order accepted. No exchange order was sent.".to_string(),
    };

    append_order_log(&state.config.crypto_order_log_path, &normalized, &response)?;
    Ok(response)
}

pub fn list_order_logs(state: &AppState) -> ApiResult<Vec<Value>> {
    let path = &state.config.crypto_order_log_path;
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| file_error("crypto_order_log_read_failed", error))?;
    let mut rows = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    rows.reverse();
    rows.truncate(50);
    Ok(rows)
}

fn default_instruments() -> Vec<CryptoInstrument> {
    vec![
        instrument("BTCUSDT", "Bitcoin", "spot", None),
        instrument("ETHUSDT", "Ethereum", "spot", None),
        instrument("SOLUSDT", "Solana", "spot", None),
        instrument("XRPUSDT", "XRP", "spot", None),
        instrument("BTCUSDT", "Bitcoin Perpetual", "futures", Some(20)),
        instrument("ETHUSDT", "Ethereum Perpetual", "futures", Some(20)),
        instrument("SOLUSDT", "Solana Perpetual", "futures", Some(10)),
        instrument("BNBUSDT", "BNB Perpetual", "futures", Some(10)),
    ]
}

fn instrument(
    symbol: &str,
    name: &str,
    market_type: &str,
    max_leverage: Option<u32>,
) -> CryptoInstrument {
    CryptoInstrument {
        symbol: symbol.to_string(),
        name: name.to_string(),
        venue: "Binance".to_string(),
        market_type: market_type.to_string(),
        quote_asset: "USDT".to_string(),
        max_leverage,
    }
}

fn normalize_order_request(request: CryptoOrderRequest) -> ApiResult<CryptoOrderRequest> {
    let market_type = normalize_market_type(&request.market_type)?;
    let side = request.side.trim().to_ascii_lowercase();
    if side != "buy" && side != "sell" && side != "long" && side != "short" {
        return validation_error(
            "invalid_crypto_order_side",
            "Side must be buy, sell, long, or short.",
        );
    }
    if market_type == "spot" && (side == "long" || side == "short") {
        return validation_error("invalid_spot_side", "Spot orders only support buy or sell.");
    }

    let symbol = normalize_symbol(&request.symbol)?;
    if request.quantity <= 0.0 || !request.quantity.is_finite() {
        return validation_error(
            "invalid_crypto_quantity",
            "Quantity must be greater than zero.",
        );
    }
    if request.price <= 0.0 || !request.price.is_finite() {
        return validation_error("invalid_crypto_price", "Price must be greater than zero.");
    }

    let leverage = if market_type == "futures" {
        Some(request.leverage.unwrap_or(1).clamp(1, 20))
    } else {
        None
    };

    Ok(CryptoOrderRequest {
        market_type,
        side,
        symbol,
        quantity: request.quantity,
        price: request.price,
        leverage,
    })
}

fn validate_crypto_risk(state: &AppState, request: &CryptoOrderRequest) -> ApiResult<()> {
    let amount = notional(request);
    if amount > state.config.max_crypto_order_amount_usdt {
        return validation_error(
            "max_crypto_order_amount_exceeded",
            "Order notional exceeds MAX_CRYPTO_ORDER_AMOUNT_USDT.",
        );
    }

    Ok(())
}

fn normalize_market_type(market_type: &str) -> ApiResult<String> {
    let market_type = market_type.trim().to_ascii_lowercase();
    if market_type == "spot" || market_type == "futures" {
        return Ok(market_type);
    }

    validation_error(
        "invalid_crypto_market_type",
        "Market type must be spot or futures.",
    )
}

fn normalize_symbol(symbol: &str) -> ApiResult<String> {
    let symbol = symbol.trim().to_ascii_uppercase();
    if symbol.len() >= 6
        && symbol.len() <= 20
        && symbol.chars().all(|char| char.is_ascii_alphanumeric())
    {
        return Ok(symbol);
    }

    validation_error(
        "invalid_crypto_symbol",
        "Symbol must be an exchange symbol such as BTCUSDT.",
    )
}

fn notional(request: &CryptoOrderRequest) -> f64 {
    request.quantity * request.price
}

fn append_order_log(
    path: &str,
    request: &CryptoOrderRequest,
    response: &CryptoOrderResponse,
) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("crypto_order_log_dir_failed", error))?;
    }

    let entry = CryptoOrderLogEntry {
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        request,
        response,
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| file_error("crypto_order_log_serialize_failed", error))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| file_error("crypto_order_log_open_failed", error))?;
    writeln!(file, "{line}").map_err(|error| file_error("crypto_order_log_write_failed", error))
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
