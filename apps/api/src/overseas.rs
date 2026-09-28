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
    kis, risk_settings,
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct OverseasInstrument {
    pub symbol: String,
    pub name: String,
    pub exchange_code: String,
    pub order_exchange_code: String,
    pub market: String,
    pub currency: String,
}

#[derive(Serialize)]
pub struct OverseasQuote {
    pub symbol: String,
    pub name: String,
    pub exchange_code: String,
    pub order_exchange_code: String,
    pub market: String,
    pub currency: String,
    pub output: Option<Value>,
}

#[derive(Deserialize, Serialize)]
pub struct OverseasOrderRequest {
    pub side: String,
    pub symbol: String,
    pub exchange_code: String,
    pub quantity: u32,
    pub price: f64,
}

#[derive(Serialize)]
pub struct OverseasOrderResponse {
    pub accepted: bool,
    pub mode: String,
    pub side: String,
    pub symbol: String,
    pub exchange_code: String,
    pub quantity: u32,
    pub price: f64,
    pub order_amount_usd: f64,
    pub status: String,
    pub message: String,
}

#[derive(Serialize)]
struct OverseasOrderLogEntry<'a> {
    timestamp_unix: u64,
    request: &'a OverseasOrderRequest,
    response: &'a OverseasOrderResponse,
}

pub fn instruments() -> Vec<OverseasInstrument> {
    default_instruments()
}

pub async fn quote(
    state: &AppState,
    exchange_code: &str,
    symbol: &str,
) -> ApiResult<OverseasQuote> {
    kis::ensure_kis_configured(&state.config)?;

    let symbol = normalize_symbol(symbol)?;
    let exchange_code = normalize_price_exchange_code(exchange_code)?;
    let instrument = find_instrument(&exchange_code, &symbol).unwrap_or_else(|| {
        instrument(
            &symbol,
            &symbol,
            &exchange_code,
            order_exchange_code(&exchange_code),
        )
    });
    let params = [
        ("AUTH", ""),
        ("EXCD", exchange_code.as_str()),
        ("SYMB", symbol.as_str()),
    ];

    let value = kis::kis_get(
        state,
        "/uapi/overseas-price/v1/quotations/price",
        "HHDFS00000300",
        &params,
    )
    .await?;
    let response = kis::to_kis_response(value);

    Ok(OverseasQuote {
        symbol,
        name: instrument.name,
        exchange_code,
        order_exchange_code: instrument.order_exchange_code,
        market: instrument.market,
        currency: instrument.currency,
        output: response.output,
    })
}

pub async fn place_order(
    state: &AppState,
    request: OverseasOrderRequest,
) -> ApiResult<OverseasOrderResponse> {
    let normalized = normalize_order_request(request)?;
    validate_overseas_risk(state, &normalized)?;

    let live_mode = matches!(state.config.trading_mode.as_str(), "live" | "real")
        && state.config.live_trading_enabled;
    if live_mode {
        return validation_error(
            "overseas_live_order_not_implemented",
            "Overseas live order routing is intentionally not enabled yet. Add KIS signed overseas order support after paper validation.",
        );
    }

    let response = OverseasOrderResponse {
        accepted: true,
        mode: "paper".to_string(),
        side: normalized.side.clone(),
        symbol: normalized.symbol.clone(),
        exchange_code: normalized.exchange_code.clone(),
        quantity: normalized.quantity,
        price: normalized.price,
        order_amount_usd: order_amount(&normalized),
        status: "paper_accepted".to_string(),
        message: "Paper overseas stock order accepted. No broker order was sent.".to_string(),
    };

    append_order_log(
        &state.config.overseas_order_log_path,
        &normalized,
        &response,
    )?;
    Ok(response)
}

pub fn list_order_logs(state: &AppState) -> ApiResult<Vec<Value>> {
    let path = &state.config.overseas_order_log_path;
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| file_error("overseas_order_log_read_failed", error))?;
    let mut rows = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    rows.reverse();
    rows.truncate(50);
    Ok(rows)
}

fn default_instruments() -> Vec<OverseasInstrument> {
    vec![
        instrument("AAPL", "Apple", "NAS", "NASD"),
        instrument("MSFT", "Microsoft", "NAS", "NASD"),
        instrument("NVDA", "NVIDIA", "NAS", "NASD"),
        instrument("TSLA", "Tesla", "NAS", "NASD"),
        instrument("AMZN", "Amazon", "NAS", "NASD"),
        instrument("GOOGL", "Alphabet", "NAS", "NASD"),
        instrument("META", "Meta Platforms", "NAS", "NASD"),
        instrument("BRK.B", "Berkshire Hathaway", "NYS", "NYSE"),
        instrument("JPM", "JPMorgan Chase", "NYS", "NYSE"),
        instrument("SPY", "SPDR S&P 500 ETF", "AMS", "AMEX"),
    ]
}

fn instrument(
    symbol: &str,
    name: &str,
    exchange_code: &str,
    order_exchange_code: &str,
) -> OverseasInstrument {
    OverseasInstrument {
        symbol: symbol.to_string(),
        name: name.to_string(),
        exchange_code: exchange_code.to_string(),
        order_exchange_code: order_exchange_code.to_string(),
        market: market_name(exchange_code).to_string(),
        currency: "USD".to_string(),
    }
}

fn find_instrument(exchange_code: &str, symbol: &str) -> Option<OverseasInstrument> {
    default_instruments().into_iter().find(|instrument| {
        instrument.exchange_code == exchange_code && instrument.symbol.eq_ignore_ascii_case(symbol)
    })
}

fn normalize_order_request(request: OverseasOrderRequest) -> ApiResult<OverseasOrderRequest> {
    let side = request.side.trim().to_ascii_lowercase();
    if side != "buy" && side != "sell" {
        return validation_error("invalid_overseas_order_side", "Side must be buy or sell.");
    }

    let symbol = normalize_symbol(&request.symbol)?;
    let exchange_code = normalize_order_exchange_code(&request.exchange_code)?;
    if request.quantity == 0 {
        return validation_error(
            "invalid_overseas_quantity",
            "Quantity must be greater than zero.",
        );
    }
    if request.price <= 0.0 || !request.price.is_finite() {
        return validation_error("invalid_overseas_price", "Price must be greater than zero.");
    }

    Ok(OverseasOrderRequest {
        side,
        symbol,
        exchange_code,
        quantity: request.quantity,
        price: request.price,
    })
}

fn validate_overseas_risk(state: &AppState, request: &OverseasOrderRequest) -> ApiResult<()> {
    let amount = order_amount(request);
    if amount > risk_settings::get(state)?.max_overseas_order_amount_usd {
        return validation_error(
            "max_overseas_order_amount_exceeded",
            "Order amount exceeds MAX_OVERSEAS_ORDER_AMOUNT_USD.",
        );
    }

    Ok(())
}

fn normalize_symbol(symbol: &str) -> ApiResult<String> {
    let symbol = symbol.trim().to_ascii_uppercase();
    if !symbol.is_empty()
        && symbol.len() <= 12
        && symbol
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || char == '.' || char == '-')
    {
        return Ok(symbol);
    }

    validation_error(
        "invalid_overseas_symbol",
        "Symbol must be a US stock ticker such as AAPL.",
    )
}

fn normalize_price_exchange_code(exchange_code: &str) -> ApiResult<String> {
    match exchange_code.trim().to_ascii_uppercase().as_str() {
        "NAS" | "NASD" => Ok("NAS".to_string()),
        "NYS" | "NYSE" => Ok("NYS".to_string()),
        "AMS" | "AMEX" => Ok("AMS".to_string()),
        _ => validation_error(
            "invalid_overseas_exchange",
            "Exchange must be NAS, NYS, or AMS.",
        ),
    }
}

fn normalize_order_exchange_code(exchange_code: &str) -> ApiResult<String> {
    match exchange_code.trim().to_ascii_uppercase().as_str() {
        "NAS" | "NASD" => Ok("NASD".to_string()),
        "NYS" | "NYSE" => Ok("NYSE".to_string()),
        "AMS" | "AMEX" => Ok("AMEX".to_string()),
        _ => validation_error(
            "invalid_overseas_exchange",
            "Exchange must be NASD, NYSE, or AMEX.",
        ),
    }
}

fn order_exchange_code(price_exchange_code: &str) -> &str {
    match price_exchange_code {
        "NYS" => "NYSE",
        "AMS" => "AMEX",
        _ => "NASD",
    }
}

fn market_name(exchange_code: &str) -> &str {
    match exchange_code {
        "NYS" => "NYSE",
        "AMS" => "AMEX",
        _ => "NASDAQ",
    }
}

fn order_amount(request: &OverseasOrderRequest) -> f64 {
    f64::from(request.quantity) * request.price
}

fn append_order_log(
    path: &str,
    request: &OverseasOrderRequest,
    response: &OverseasOrderResponse,
) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("overseas_order_log_dir_failed", error))?;
    }

    let entry = OverseasOrderLogEntry {
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        request,
        response,
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| file_error("overseas_order_log_serialize_failed", error))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| file_error("overseas_order_log_open_failed", error))?;
    writeln!(file, "{line}").map_err(|error| file_error("overseas_order_log_write_failed", error))
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
