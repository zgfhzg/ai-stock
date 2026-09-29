use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{
    error::{ApiError, ApiResult},
    kis,
    orders::{self, OrderRequest, OrderResponse},
    state::AppState,
    strategy::{self, ProposalRequest, ProposalResponse},
    watchlist::{self, WatchlistItem},
};

#[derive(Deserialize)]
pub struct AutoRunRequest {
    pub execute: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoRunResponse {
    pub mode: String,
    pub executed: bool,
    pub summary: AutoRunSummary,
    pub decisions: Vec<AutoDecision>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoRunSummary {
    pub total: usize,
    pub buy: usize,
    pub sell: usize,
    pub hold: usize,
    pub skipped: usize,
    pub orders: usize,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoDecision {
    pub symbol: String,
    pub name: String,
    pub action: String,
    pub confidence: f64,
    pub reason: String,
    pub current_price: Option<u64>,
    pub previous_change: Option<i64>,
    pub previous_change_rate: Option<String>,
    pub order_submitted: bool,
    pub quantity: u32,
    pub order_amount_krw: u64,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoMonitorStartRequest {
    pub interval_seconds: Option<u64>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoMonitorSettings {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub updated_at_unix: u64,
}

#[derive(Clone, Serialize)]
pub struct AutoMonitorStatus {
    pub running: bool,
    pub interval_seconds: u64,
    pub last_started_at_unix: Option<u64>,
    pub last_stopped_at_unix: Option<u64>,
    pub last_check_at_unix: Option<u64>,
    pub next_check_at_unix: Option<u64>,
    pub last_error: Option<String>,
    pub consecutive_error_count: u32,
    pub last_response: Option<AutoRunResponse>,
}

pub struct AutoMonitorRuntime {
    handle: Option<JoinHandle<()>>,
    status: AutoMonitorStatus,
}

impl AutoMonitorRuntime {
    pub fn new(interval_seconds: u64) -> Self {
        Self {
            handle: None,
            status: AutoMonitorStatus {
                running: false,
                interval_seconds: interval_seconds.clamp(10, 3600),
                last_started_at_unix: None,
                last_stopped_at_unix: None,
                last_check_at_unix: None,
                next_check_at_unix: None,
                last_error: None,
                consecutive_error_count: 0,
                last_response: None,
            },
        }
    }
}

#[derive(Clone, Default)]
struct HoldingInfo {
    quantity: u32,
    average_price: u64,
    current_value: u64,
    profit_rate: Option<f64>,
}

#[derive(Serialize)]
struct AutoRunLogEntry<'a> {
    timestamp_unix: u64,
    response: &'a AutoRunResponse,
}

pub async fn run_once(state: &AppState, request: AutoRunRequest) -> ApiResult<AutoRunResponse> {
    let items = watchlist::list(state)?;
    if items.is_empty() {
        return validation_error("empty_watchlist", "Watchlist is empty.");
    }

    let execute = request.execute.unwrap_or(false) && state.config.auto_trade_mode == "paper_auto";
    let holdings = load_holdings(state).await?;
    let risk = crate::risk_settings::get(state)?;
    let portfolio_value = holdings
        .values()
        .map(|holding| holding.current_value)
        .sum::<u64>();
    let mut remaining_budget = risk.auto_trading_budget_krw.saturating_sub(portfolio_value);
    let mut decisions = Vec::with_capacity(items.len());
    let mut orders_count = 0;

    // KIS applies the same short-window rate limit to balance and quote requests.
    sleep(Duration::from_millis(1200)).await;

    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            sleep(Duration::from_millis(1200)).await;
        }

        let decision = build_decision(
            state,
            item,
            holdings.get(&item.symbol),
            execute,
            &risk,
            &mut remaining_budget,
        )
        .await?;
        if decision.order_submitted {
            orders_count += 1;
        }
        decisions.push(decision);
    }

    let summary = summarize(&decisions, orders_count);
    let response = AutoRunResponse {
        mode: state.config.auto_trade_mode.clone(),
        executed: execute,
        summary,
        decisions,
    };

    append_run_log(&state.config.auto_decision_log_path, &response)?;
    Ok(response)
}

pub fn list_logs(state: &AppState) -> ApiResult<Vec<Value>> {
    let path = &state.config.auto_decision_log_path;
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("auto_log_read_failed", error))?;
    let mut rows = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    rows.reverse();
    rows.truncate(20);
    Ok(rows)
}

pub async fn monitor_status(state: &AppState) -> AutoMonitorStatus {
    state.auto_monitor.lock().await.status.clone()
}

pub async fn restore_monitor(state: &AppState) -> ApiResult<Option<AutoMonitorStatus>> {
    let settings = read_monitor_settings(state)?;
    if !settings.enabled {
        state.auto_monitor.lock().await.status.interval_seconds = settings.interval_seconds;
        return Ok(None);
    }
    Ok(Some(
        start_monitor(
            state,
            AutoMonitorStartRequest {
                interval_seconds: Some(settings.interval_seconds),
            },
        )
        .await?,
    ))
}

pub async fn start_monitor(
    state: &AppState,
    request: AutoMonitorStartRequest,
) -> ApiResult<AutoMonitorStatus> {
    if state.config.auto_trade_mode != "paper_auto" {
        return validation_error(
            "auto_trading_disabled",
            "AUTO_TRADE_MODE must be paper_auto before starting AI monitoring.",
        );
    }
    if watchlist::list(state)?.is_empty() {
        return validation_error("empty_watchlist", "Watchlist is empty.");
    }

    let interval_seconds = request.interval_seconds.unwrap_or(30).clamp(10, 3600);
    let now = unix_now();
    write_monitor_settings(
        state,
        &AutoMonitorSettings {
            enabled: true,
            interval_seconds,
            updated_at_unix: now,
        },
    )?;

    let mut runtime = state.auto_monitor.lock().await;
    if let Some(handle) = runtime.handle.take() {
        handle.abort();
    }
    runtime.status.running = true;
    runtime.status.interval_seconds = interval_seconds;
    runtime.status.last_started_at_unix = Some(now);
    runtime.status.last_stopped_at_unix = None;
    runtime.status.next_check_at_unix = Some(now);
    runtime.status.last_error = None;
    runtime.status.consecutive_error_count = 0;

    let worker_state = state.clone();
    runtime.handle = Some(tokio::spawn(async move {
        monitor_loop(worker_state, interval_seconds).await;
    }));
    Ok(runtime.status.clone())
}

pub async fn stop_monitor(state: &AppState) -> AutoMonitorStatus {
    let mut runtime = state.auto_monitor.lock().await;
    if let Some(handle) = runtime.handle.take() {
        handle.abort();
    }
    runtime.status.running = false;
    runtime.status.last_stopped_at_unix = Some(unix_now());
    runtime.status.next_check_at_unix = None;
    let _ = write_monitor_settings(
        state,
        &AutoMonitorSettings {
            enabled: false,
            interval_seconds: runtime.status.interval_seconds,
            updated_at_unix: unix_now(),
        },
    );
    runtime.status.clone()
}

async fn monitor_loop(state: AppState, interval_seconds: u64) {
    loop {
        let started_at = unix_now();
        {
            let mut runtime = state.auto_monitor.lock().await;
            runtime.status.last_check_at_unix = Some(started_at);
            runtime.status.next_check_at_unix = Some(started_at + interval_seconds);
        }

        let result = run_once(
            &state,
            AutoRunRequest {
                execute: Some(true),
            },
        )
        .await;
        let should_continue = {
            let mut runtime = state.auto_monitor.lock().await;
            match result {
                Ok(response) => {
                    runtime.status.last_response = Some(response);
                    runtime.status.last_error = None;
                    runtime.status.consecutive_error_count = 0;
                    true
                }
                Err((_, Json(error))) => {
                    runtime.status.last_error = Some(error.message);
                    runtime.status.consecutive_error_count += 1;
                    if runtime.status.consecutive_error_count >= 3 {
                        runtime.status.running = false;
                        runtime.status.next_check_at_unix = None;
                        let _ = write_monitor_settings(
                            &state,
                            &AutoMonitorSettings {
                                enabled: false,
                                interval_seconds,
                                updated_at_unix: unix_now(),
                            },
                        );
                        false
                    } else {
                        true
                    }
                }
            }
        };
        if !should_continue {
            break;
        }
        sleep(Duration::from_secs(interval_seconds)).await;
    }
}

fn read_monitor_settings(state: &AppState) -> ApiResult<AutoMonitorSettings> {
    let path = &state.config.auto_monitor_settings_path;
    if !Path::new(path).exists() {
        return Ok(AutoMonitorSettings {
            enabled: false,
            interval_seconds: 30,
            updated_at_unix: 0,
        });
    }
    let content = fs::read_to_string(path)
        .map_err(|error| file_error("auto_monitor_settings_read_failed", error))?;
    serde_json::from_str(&content)
        .map_err(|error| file_error("auto_monitor_settings_parse_failed", error))
}

fn write_monitor_settings(state: &AppState, settings: &AutoMonitorSettings) -> ApiResult<()> {
    let path = &state.config.auto_monitor_settings_path;
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("auto_monitor_settings_dir_failed", error))?;
    }
    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| file_error("auto_monitor_settings_serialize_failed", error))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| file_error("auto_monitor_settings_write_failed", error))
}

async fn build_decision(
    state: &AppState,
    item: &WatchlistItem,
    holding: Option<&HoldingInfo>,
    execute: bool,
    risk: &crate::risk_settings::RiskSettings,
    remaining_budget: &mut u64,
) -> ApiResult<AutoDecision> {
    let quote = match kis::get_price(state, &item.symbol).await {
        Ok(quote) => quote,
        Err((_, Json(error))) => {
            return Ok(AutoDecision {
                symbol: item.symbol.clone(),
                name: item.name.clone(),
                action: "skip".to_string(),
                confidence: 0.0,
                reason: error.message,
                current_price: None,
                previous_change: None,
                previous_change_rate: None,
                order_submitted: false,
                quantity: 0,
                order_amount_krw: 0,
                skip_reason: Some("quote_unavailable".to_string()),
            });
        }
    };

    let output = quote.output.as_ref();
    let current_price = output
        .and_then(|value| value.get("stck_prpr"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok());
    let previous_change = output
        .and_then(|value| value.get("prdy_vrss"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok());
    let previous_change_rate = output
        .and_then(|value| value.get("prdy_ctrt"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let previous_change_rate_number = previous_change_rate
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok());

    let proposal = strategy::proposal(
        state,
        &ProposalRequest {
            symbol: item.symbol.clone(),
            name: Some(item.name.clone()),
            current_price,
            previous_change,
            previous_change_rate: previous_change_rate_number,
            holding_quantity: holding.map(|value| value.quantity),
            average_purchase_price: holding.map(|value| value.average_price),
            evaluation_profit_rate: holding.and_then(|value| value.profit_rate),
        },
    )
    .await;

    let mut decision = proposal_to_decision(
        item,
        &proposal,
        current_price,
        previous_change,
        previous_change_rate,
    );

    let quantity = calculate_quantity(&proposal, current_price, holding, risk, *remaining_budget);
    decision.quantity = quantity;
    decision.order_amount_krw = current_price.unwrap_or_default() * u64::from(quantity);

    if execute && should_submit_order(state, &proposal, current_price) && quantity > 0 {
        let order = submit_order(state, &proposal, item, current_price.unwrap(), quantity).await?;
        decision.order_submitted = order.accepted;
        if order.accepted && proposal.action == "buy" {
            *remaining_budget = remaining_budget.saturating_sub(order.order_amount_krw);
        }
        if !order.accepted {
            decision.skip_reason = Some("order_rejected".to_string());
        }
    } else if proposal.action != "hold" {
        decision.skip_reason = Some(
            if execute {
                if quantity == 0 {
                    "budget_or_holding_limit"
                } else {
                    "risk_or_confidence_check_failed"
                }
            } else {
                "recommendation_only_mode"
            }
            .to_string(),
        );
    }

    Ok(decision)
}

fn proposal_to_decision(
    item: &WatchlistItem,
    proposal: &ProposalResponse,
    current_price: Option<u64>,
    previous_change: Option<i64>,
    previous_change_rate: Option<String>,
) -> AutoDecision {
    AutoDecision {
        symbol: item.symbol.clone(),
        name: item.name.clone(),
        action: proposal.action.clone(),
        confidence: proposal.confidence,
        reason: proposal.reason.clone(),
        current_price,
        previous_change,
        previous_change_rate,
        order_submitted: false,
        quantity: 0,
        order_amount_krw: 0,
        skip_reason: None,
    }
}

fn should_submit_order(
    state: &AppState,
    proposal: &ProposalResponse,
    current_price: Option<u64>,
) -> bool {
    matches!(proposal.action.as_str(), "buy" | "sell")
        && proposal.confidence >= state.config.auto_min_confidence
        && current_price.is_some()
}

async fn submit_order(
    state: &AppState,
    proposal: &ProposalResponse,
    item: &WatchlistItem,
    price: u64,
    quantity: u32,
) -> ApiResult<OrderResponse> {
    orders::place(
        state,
        OrderRequest {
            side: proposal.action.clone(),
            symbol: item.symbol.clone(),
            quantity,
            price,
        },
    )
    .await
}

fn calculate_quantity(
    proposal: &ProposalResponse,
    current_price: Option<u64>,
    holding: Option<&HoldingInfo>,
    risk: &crate::risk_settings::RiskSettings,
    remaining_budget: u64,
) -> u32 {
    let Some(price) = current_price.filter(|price| *price > 0) else {
        return 0;
    };

    if proposal.action == "sell" {
        let held = holding.map(|value| value.quantity).unwrap_or_default();
        let max_quantity = risk.max_order_amount_krw / price;
        return held.min(u32::try_from(max_quantity).unwrap_or(u32::MAX));
    }

    if proposal.action != "buy" {
        return 0;
    }

    let current_value = holding.map(|value| value.current_value).unwrap_or_default();
    let position_limit = (risk.auto_trading_budget_krw as f64 * risk.max_position_ratio) as u64;
    let position_room = position_limit.saturating_sub(current_value);
    let available = remaining_budget
        .min(risk.max_order_amount_krw)
        .min(risk.max_daily_auto_order_amount_krw_per_symbol)
        .min(position_room);
    u32::try_from(available / price).unwrap_or(u32::MAX)
}

async fn load_holdings(state: &AppState) -> ApiResult<HashMap<String, HoldingInfo>> {
    let balance = kis::get_balance(state).await?;
    let rows = balance
        .output1
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let symbol = row.get("pdno")?.as_str()?.to_string();
            let parse_u64 = |key: &str| {
                row.get(key)
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or_default()
            };
            let quantity = u32::try_from(parse_u64("hldg_qty")).unwrap_or(u32::MAX);
            Some((
                symbol,
                HoldingInfo {
                    quantity,
                    average_price: parse_u64("pchs_avg_pric"),
                    current_value: parse_u64("evlu_amt"),
                    profit_rate: row
                        .get("evlu_pfls_rt")
                        .and_then(Value::as_str)
                        .and_then(|value| value.parse::<f64>().ok()),
                },
            ))
        })
        .collect())
}

fn summarize(decisions: &[AutoDecision], orders: usize) -> AutoRunSummary {
    AutoRunSummary {
        total: decisions.len(),
        buy: decisions
            .iter()
            .filter(|decision| decision.action == "buy")
            .count(),
        sell: decisions
            .iter()
            .filter(|decision| decision.action == "sell")
            .count(),
        hold: decisions
            .iter()
            .filter(|decision| decision.action == "hold")
            .count(),
        skipped: decisions
            .iter()
            .filter(|decision| decision.action == "skip")
            .count(),
        orders,
    }
}

fn append_run_log(path: &str, response: &AutoRunResponse) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("auto_log_dir_failed", error))?;
    }

    let entry = AutoRunLogEntry {
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        response,
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| file_error("auto_log_serialize_failed", error))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| file_error("auto_log_open_failed", error))?;
    writeln!(file, "{line}").map_err(|error| file_error("auto_log_write_failed", error))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk_settings::RiskSettings;

    fn risk() -> RiskSettings {
        RiskSettings {
            auto_trading_budget_krw: 500_000,
            max_order_amount_krw: 100_000,
            max_daily_auto_order_amount_krw_per_symbol: 100_000,
            max_position_ratio: 0.2,
            daily_max_loss_ratio: 0.03,
            daily_max_order_count: 20,
            max_overseas_order_amount_usd: 100.0,
            max_crypto_order_amount_usdt: 100.0,
        }
    }

    fn proposal(action: &str) -> ProposalResponse {
        ProposalResponse {
            action: action.to_string(),
            confidence: 0.8,
            reason: "test".to_string(),
            live_order_allowed: false,
        }
    }

    #[test]
    fn buy_quantity_respects_order_and_position_limits() {
        assert_eq!(
            calculate_quantity(&proposal("buy"), Some(30_000), None, &risk(), 500_000),
            3
        );
    }

    #[test]
    fn buy_quantity_is_zero_when_budget_is_exhausted() {
        assert_eq!(
            calculate_quantity(&proposal("buy"), Some(30_000), None, &risk(), 0),
            0
        );
    }

    #[test]
    fn sell_quantity_never_exceeds_holding() {
        let holding = HoldingInfo {
            quantity: 2,
            ..HoldingInfo::default()
        };
        assert_eq!(
            calculate_quantity(
                &proposal("sell"),
                Some(30_000),
                Some(&holding),
                &risk(),
                500_000,
            ),
            2
        );
    }
}
