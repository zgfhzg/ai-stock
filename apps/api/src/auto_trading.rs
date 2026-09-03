use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{sleep, Duration};

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

#[derive(Serialize)]
pub struct AutoRunResponse {
    pub mode: String,
    pub executed: bool,
    pub summary: AutoRunSummary,
    pub decisions: Vec<AutoDecision>,
}

#[derive(Serialize)]
pub struct AutoRunSummary {
    pub total: usize,
    pub buy: usize,
    pub sell: usize,
    pub hold: usize,
    pub skipped: usize,
    pub orders: usize,
}

#[derive(Clone, Serialize)]
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
    pub skip_reason: Option<String>,
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
    let mut decisions = Vec::with_capacity(items.len());
    let mut orders_count = 0;

    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            sleep(Duration::from_millis(500)).await;
        }

        let decision = build_decision(state, item, execute).await?;
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

async fn build_decision(
    state: &AppState,
    item: &WatchlistItem,
    execute: bool,
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

    let proposal = strategy::proposal(
        state,
        &ProposalRequest {
            symbol: item.symbol.clone(),
            name: Some(item.name.clone()),
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

    if execute && should_submit_order(state, &proposal, current_price) {
        let order = submit_order(state, &proposal, item, current_price.unwrap()).await?;
        decision.order_submitted = order.accepted;
        if !order.accepted {
            decision.skip_reason = Some("order_rejected".to_string());
        }
    } else if proposal.action != "hold" {
        decision.skip_reason = Some(
            if execute {
                "risk_or_confidence_check_failed"
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
) -> ApiResult<OrderResponse> {
    orders::place(
        state,
        OrderRequest {
            side: proposal.action.clone(),
            symbol: item.symbol.clone(),
            quantity: 1,
            price,
        },
    )
    .await
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
