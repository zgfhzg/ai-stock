use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{sleep, Duration};

use crate::{
    error::ApiResult,
    kis,
    orders::{self, OrderRequest},
    state::AppState,
    trading_rules::{self, RuleTrigger, TradingRule},
};

#[derive(Deserialize)]
pub struct RuleCheckRequest {
    pub execute: Option<bool>,
}

#[derive(Serialize)]
pub struct RuleCheckResponse {
    pub mode: String,
    pub executed: bool,
    pub summary: RuleCheckSummary,
    pub results: Vec<RuleCheckResult>,
}

#[derive(Serialize)]
pub struct RuleCheckSummary {
    pub total: usize,
    pub matched: usize,
    pub waiting: usize,
    pub cooldown: usize,
    pub skipped: usize,
    pub orders: usize,
}

#[derive(Clone, Serialize)]
pub struct RuleCheckResult {
    pub rule_id: String,
    pub symbol: String,
    pub name: String,
    pub trigger: RuleTrigger,
    pub action: String,
    pub status: String,
    pub reason: String,
    pub current_price: Option<u64>,
    pub target_price: u64,
    pub quantity: u32,
    pub order_submitted: bool,
    pub cooldown_until_unix: Option<u64>,
}

#[derive(Serialize)]
struct RuleCheckLogEntry<'a> {
    timestamp_unix: u64,
    response: &'a RuleCheckResponse,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct RuleMonitorState {
    rules: HashMap<String, RuleState>,
}

#[derive(Clone, Deserialize, Serialize)]
struct RuleState {
    last_triggered_at_unix: u64,
}

pub async fn check_once(
    state: &AppState,
    request: RuleCheckRequest,
) -> ApiResult<RuleCheckResponse> {
    let rules = trading_rules::list(state)?;
    let execute = request.execute.unwrap_or(false) && state.config.auto_trade_mode == "paper_auto";
    let now = unix_now();
    let mut monitor_state = read_monitor_state(&state.config.auto_rule_state_path)?;
    let mut results = Vec::with_capacity(rules.len());
    let mut orders_count = 0;

    for (index, rule) in rules.iter().enumerate() {
        if index > 0 {
            sleep(Duration::from_millis(1200)).await;
        }

        let result = check_rule(state, rule, execute, now, &monitor_state).await?;
        if result.order_submitted {
            orders_count += 1;
        }
        if matches!(result.status.as_str(), "matched" | "order_submitted") {
            monitor_state.rules.insert(
                rule.id.clone(),
                RuleState {
                    last_triggered_at_unix: now,
                },
            );
        }
        results.push(result);
    }

    prune_monitor_state(&mut monitor_state, &rules);
    write_monitor_state(&state.config.auto_rule_state_path, &monitor_state)?;

    let response = RuleCheckResponse {
        mode: state.config.auto_trade_mode.clone(),
        executed: execute,
        summary: summarize(&results, orders_count),
        results,
    };

    append_check_log(&state.config.auto_rule_check_log_path, &response)?;
    Ok(response)
}

async fn check_rule(
    state: &AppState,
    rule: &TradingRule,
    execute: bool,
    now: u64,
    monitor_state: &RuleMonitorState,
) -> ApiResult<RuleCheckResult> {
    let action = rule_action(&rule.trigger);
    if !rule.enabled {
        return Ok(result(
            rule,
            action,
            "skipped",
            "비활성 규칙이라 점검하지 않았습니다.",
            None,
            false,
            None,
        ));
    }

    let quote = match kis::get_price(state, &rule.symbol).await {
        Ok(quote) => quote,
        Err((_, Json(error))) => {
            return Ok(result(
                rule,
                action,
                "skipped",
                &error.message,
                None,
                false,
                None,
            ));
        }
    };
    let current_price = quote
        .output
        .as_ref()
        .and_then(|value| value.get("stck_prpr"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok());

    let Some(current_price) = current_price else {
        return Ok(result(
            rule,
            action,
            "skipped",
            "현재가를 읽지 못해 규칙을 판정하지 않았습니다.",
            None,
            false,
            None,
        ));
    };

    if !is_matched(&rule.trigger, current_price, rule.target_price) {
        return Ok(result(
            rule,
            action,
            "waiting",
            &format_waiting_reason(rule, current_price),
            Some(current_price),
            false,
            None,
        ));
    }

    if let Some(cooldown_until_unix) = cooldown_until(state, rule, now, monitor_state) {
        return Ok(result(
            rule,
            action,
            "cooldown",
            "조건은 충족됐지만 쿨다운 중이라 이번 점검에서는 제외했습니다.",
            Some(current_price),
            false,
            Some(cooldown_until_unix),
        ));
    }

    if !execute {
        return Ok(result(
            rule,
            action,
            "matched",
            &format!(
                "{} 조건이 충족됐지만 추천 모드라 주문하지 않았습니다.",
                format_trigger(&rule.trigger)
            ),
            Some(current_price),
            false,
            None,
        ));
    }

    let order = orders::place(
        state,
        OrderRequest {
            side: action.to_string(),
            symbol: rule.symbol.clone(),
            quantity: rule.quantity,
            price: current_price,
        },
    )
    .await?;

    Ok(result(
        rule,
        action,
        if order.accepted {
            "order_submitted"
        } else {
            "order_rejected"
        },
        order
            .kis
            .msg1
            .as_deref()
            .unwrap_or("주문 응답을 받았습니다."),
        Some(current_price),
        order.accepted,
        None,
    ))
}

fn result(
    rule: &TradingRule,
    action: &str,
    status: &str,
    reason: &str,
    current_price: Option<u64>,
    order_submitted: bool,
    cooldown_until_unix: Option<u64>,
) -> RuleCheckResult {
    RuleCheckResult {
        rule_id: rule.id.clone(),
        symbol: rule.symbol.clone(),
        name: rule.name.clone(),
        trigger: rule.trigger.clone(),
        action: action.to_string(),
        status: status.to_string(),
        reason: reason.to_string(),
        current_price,
        target_price: rule.target_price,
        quantity: rule.quantity,
        order_submitted,
        cooldown_until_unix,
    }
}

fn is_matched(trigger: &RuleTrigger, current_price: u64, target_price: u64) -> bool {
    match trigger {
        RuleTrigger::BuyBelow | RuleTrigger::StopLoss => current_price <= target_price,
        RuleTrigger::SellAbove | RuleTrigger::TakeProfit => current_price >= target_price,
    }
}

fn rule_action(trigger: &RuleTrigger) -> &'static str {
    match trigger {
        RuleTrigger::BuyBelow => "buy",
        RuleTrigger::SellAbove | RuleTrigger::StopLoss | RuleTrigger::TakeProfit => "sell",
    }
}

fn format_trigger(trigger: &RuleTrigger) -> &'static str {
    match trigger {
        RuleTrigger::BuyBelow => "가격 이하 매수",
        RuleTrigger::SellAbove => "가격 이상 매도",
        RuleTrigger::StopLoss => "손절",
        RuleTrigger::TakeProfit => "익절",
    }
}

fn format_waiting_reason(rule: &TradingRule, current_price: u64) -> String {
    let direction = match rule.trigger {
        RuleTrigger::BuyBelow | RuleTrigger::StopLoss => "아직 기준가보다 높습니다.",
        RuleTrigger::SellAbove | RuleTrigger::TakeProfit => "아직 기준가보다 낮습니다.",
    };
    format!(
        "{} 조건 대기 중입니다. 현재가 {}, 기준가 {}.",
        format_trigger(&rule.trigger),
        current_price,
        rule.target_price
    ) + " "
        + direction
}

fn cooldown_until(
    state: &AppState,
    rule: &TradingRule,
    now: u64,
    monitor_state: &RuleMonitorState,
) -> Option<u64> {
    let rule_state = monitor_state.rules.get(&rule.id)?;
    let cooldown_until =
        rule_state.last_triggered_at_unix + state.config.auto_rule_cooldown_seconds;

    if now < cooldown_until {
        Some(cooldown_until)
    } else {
        None
    }
}

fn summarize(results: &[RuleCheckResult], orders: usize) -> RuleCheckSummary {
    RuleCheckSummary {
        total: results.len(),
        matched: results
            .iter()
            .filter(|result| result.status == "matched" || result.status == "order_submitted")
            .count(),
        waiting: results
            .iter()
            .filter(|result| result.status == "waiting")
            .count(),
        cooldown: results
            .iter()
            .filter(|result| result.status == "cooldown")
            .count(),
        skipped: results
            .iter()
            .filter(|result| result.status == "skipped" || result.status == "order_rejected")
            .count(),
        orders,
    }
}

fn read_monitor_state(path: &str) -> ApiResult<RuleMonitorState> {
    if !Path::new(path).exists() {
        return Ok(RuleMonitorState::default());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("rule_state_read_failed", error))?;
    serde_json::from_str(&content).map_err(|error| file_error("rule_state_parse_failed", error))
}

fn write_monitor_state(path: &str, state: &RuleMonitorState) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("rule_state_dir_failed", error))?;
    }

    let content = serde_json::to_string_pretty(state)
        .map_err(|error| file_error("rule_state_serialize_failed", error))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| file_error("rule_state_write_failed", error))
}

fn prune_monitor_state(state: &mut RuleMonitorState, rules: &[TradingRule]) {
    state
        .rules
        .retain(|rule_id, _| rules.iter().any(|rule| rule.id == *rule_id));
}

fn append_check_log(path: &str, response: &RuleCheckResponse) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("rule_check_log_dir_failed", error))?;
    }

    let entry = RuleCheckLogEntry {
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        response,
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| file_error("rule_check_log_serialize_failed", error))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| file_error("rule_check_log_open_failed", error))?;
    writeln!(file, "{line}").map_err(|error| file_error("rule_check_log_write_failed", error))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn file_error(
    code: impl Into<String>,
    error: impl std::fmt::Display,
) -> (axum::http::StatusCode, Json<crate::error::ApiError>) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(crate::error::ApiError {
            code: code.into(),
            message: error.to_string(),
        }),
    )
}
