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
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{
    error::ApiResult,
    kis,
    orders::{self, OrderRequest},
    risk_settings,
    state::AppState,
    strategy::{self, ProposalRequest, ProposalResponse},
    trading_rules::{self, RuleTrigger, TradingRule},
};

#[derive(Deserialize)]
pub struct RuleCheckRequest {
    pub execute: Option<bool>,
}

#[derive(Clone, Deserialize)]
pub struct RuleMonitorStartRequest {
    pub interval_seconds: Option<u64>,
    pub execute: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RuleMonitorSettings {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub execute: bool,
    pub updated_at_unix: u64,
}

#[derive(Clone, Serialize)]
pub struct RuleMonitorStatus {
    pub running: bool,
    pub interval_seconds: u64,
    pub execute: bool,
    pub last_started_at_unix: Option<u64>,
    pub last_stopped_at_unix: Option<u64>,
    pub last_check_at_unix: Option<u64>,
    pub next_check_at_unix: Option<u64>,
    pub last_error: Option<String>,
    pub consecutive_error_count: u32,
    pub last_response: Option<RuleCheckResponse>,
}

pub struct RuleMonitorRuntime {
    handle: Option<JoinHandle<()>>,
    status: RuleMonitorStatus,
}

impl Default for RuleMonitorRuntime {
    fn default() -> Self {
        Self::new(30)
    }
}

impl RuleMonitorRuntime {
    pub fn new(interval_seconds: u64) -> Self {
        Self {
            handle: None,
            status: RuleMonitorStatus {
                running: false,
                interval_seconds: interval_seconds.clamp(10, 3600),
                execute: false,
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

#[derive(Clone, Deserialize, Serialize)]
pub struct RuleCheckResponse {
    pub mode: String,
    pub executed: bool,
    pub summary: RuleCheckSummary,
    pub results: Vec<RuleCheckResult>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RuleCheckSummary {
    pub total: usize,
    pub matched: usize,
    pub waiting: usize,
    pub cooldown: usize,
    pub skipped: usize,
    pub orders: usize,
}

#[derive(Clone, Deserialize, Serialize)]
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
    pub ai_action: Option<String>,
    pub ai_confidence: Option<f64>,
    pub ai_reason: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct RuleCheckLog {
    pub timestamp_unix: u64,
    pub response: RuleCheckResponse,
}

#[derive(Serialize)]
struct RuleCheckLogEntry<'a> {
    timestamp_unix: u64,
    response: &'a RuleCheckResponse,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct RuleMonitorState {
    rules: HashMap<String, RuleState>,
    #[serde(default)]
    symbols: HashMap<String, SymbolState>,
}

#[derive(Clone, Deserialize, Serialize)]
struct RuleState {
    last_triggered_at_unix: u64,
    #[serde(default)]
    daily_execution_date: Option<u64>,
    #[serde(default)]
    daily_execution_count: u32,
}

#[derive(Clone, Deserialize, Serialize)]
struct SymbolState {
    daily_order_date: u64,
    daily_order_amount_krw: u64,
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
            mark_rule_triggered(&mut monitor_state, rule, &result, now);
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

pub async fn monitor_status(state: &AppState) -> RuleMonitorStatus {
    state.rule_monitor.lock().await.status.clone()
}

pub fn list_logs(state: &AppState) -> ApiResult<Vec<RuleCheckLog>> {
    let path = &state.config.auto_rule_check_log_path;
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| file_error("rule_check_log_read_failed", error))?;
    let mut rows = content
        .lines()
        .filter_map(|line| serde_json::from_str::<RuleCheckLog>(line).ok())
        .collect::<Vec<_>>();
    rows.reverse();
    rows.truncate(20);
    Ok(rows)
}

pub async fn restore_monitor(state: &AppState) -> ApiResult<Option<RuleMonitorStatus>> {
    let settings = read_monitor_settings(state)?;
    if !settings.enabled {
        let mut runtime = state.rule_monitor.lock().await;
        runtime.status.interval_seconds = settings.interval_seconds;
        runtime.status.execute = settings.execute;
        return Ok(None);
    }

    let status = start_monitor(
        state,
        RuleMonitorStartRequest {
            interval_seconds: Some(settings.interval_seconds),
            execute: Some(settings.execute),
        },
    )
    .await?;
    Ok(Some(status))
}

pub async fn start_monitor(
    state: &AppState,
    request: RuleMonitorStartRequest,
) -> ApiResult<RuleMonitorStatus> {
    let interval_seconds = request.interval_seconds.unwrap_or(30).clamp(10, 3600);
    let execute = request.execute.unwrap_or(false);
    let now = unix_now();
    let worker_state = state.clone();

    write_monitor_settings(
        state,
        &RuleMonitorSettings {
            enabled: true,
            interval_seconds,
            execute,
            updated_at_unix: now,
        },
    )?;

    let mut runtime = state.rule_monitor.lock().await;
    if let Some(handle) = runtime.handle.take() {
        handle.abort();
    }

    runtime.status.running = true;
    runtime.status.interval_seconds = interval_seconds;
    runtime.status.execute = execute;
    runtime.status.last_started_at_unix = Some(now);
    runtime.status.last_stopped_at_unix = None;
    runtime.status.next_check_at_unix = Some(now);
    runtime.status.last_error = None;
    runtime.status.consecutive_error_count = 0;

    let handle = tokio::spawn(async move {
        monitor_loop(worker_state, interval_seconds, execute).await;
    });
    runtime.handle = Some(handle);

    Ok(runtime.status.clone())
}

pub async fn stop_monitor(state: &AppState) -> RuleMonitorStatus {
    let mut runtime = state.rule_monitor.lock().await;
    if let Some(handle) = runtime.handle.take() {
        handle.abort();
    }

    runtime.status.running = false;
    runtime.status.last_stopped_at_unix = Some(unix_now());
    runtime.status.next_check_at_unix = None;
    let _ = write_monitor_settings(
        state,
        &RuleMonitorSettings {
            enabled: false,
            interval_seconds: runtime.status.interval_seconds,
            execute: runtime.status.execute,
            updated_at_unix: unix_now(),
        },
    );
    runtime.status.clone()
}

async fn monitor_loop(state: AppState, interval_seconds: u64, execute: bool) {
    loop {
        let started_at = unix_now();
        {
            let mut runtime = state.rule_monitor.lock().await;
            runtime.status.running = true;
            runtime.status.last_check_at_unix = Some(started_at);
            runtime.status.next_check_at_unix = Some(started_at + interval_seconds);
        }

        let check_result = check_once(
            &state,
            RuleCheckRequest {
                execute: Some(execute),
            },
        )
        .await;
        let should_continue = {
            let mut runtime = state.rule_monitor.lock().await;
            if !runtime.status.running {
                false
            } else {
                match check_result {
                    Ok(response) => {
                        runtime.status.last_error = None;
                        runtime.status.consecutive_error_count = 0;
                        runtime.status.last_response = Some(response);
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
                                &RuleMonitorSettings {
                                    enabled: false,
                                    interval_seconds,
                                    execute,
                                    updated_at_unix: unix_now(),
                                },
                            );
                            false
                        } else {
                            true
                        }
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

fn read_monitor_settings(state: &AppState) -> ApiResult<RuleMonitorSettings> {
    let path = &state.config.auto_rule_monitor_settings_path;
    if !Path::new(path).exists() {
        return Ok(RuleMonitorSettings {
            enabled: false,
            interval_seconds: state
                .config
                .auto_rule_monitor_interval_seconds
                .clamp(10, 3600),
            execute: false,
            updated_at_unix: 0,
        });
    }

    let content = fs::read_to_string(path)
        .map_err(|error| file_error("rule_monitor_settings_read_failed", error))?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| file_error("rule_monitor_settings_parse_failed", error))?;

    Ok(RuleMonitorSettings {
        enabled: value
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        interval_seconds: value
            .get("interval_seconds")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(state.config.auto_rule_monitor_interval_seconds)
            .clamp(10, 3600),
        execute: value
            .get("execute")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        updated_at_unix: value
            .get("updated_at_unix")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    })
}

fn write_monitor_settings(state: &AppState, settings: &RuleMonitorSettings) -> ApiResult<()> {
    let path = &state.config.auto_rule_monitor_settings_path;
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| file_error("rule_monitor_settings_dir_failed", error))?;
    }

    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| file_error("rule_monitor_settings_serialize_failed", error))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| file_error("rule_monitor_settings_write_failed", error))
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
    let previous_change = quote
        .output
        .as_ref()
        .and_then(|value| value.get("prdy_vrss"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok());
    let previous_change_rate = quote
        .output
        .as_ref()
        .and_then(|value| value.get("prdy_ctrt"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok());

    let Some(current_price) = current_price else {
        return Ok(result(
            rule,
            action,
            "skipped",
            "현재가를 읽지 못해 규칙을 판정하지 않았습니다.",
            None,
            false,
            None,
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
            None,
        ));
    }

    if !execute {
        let proposal = rule_ai_proposal(
            state,
            rule,
            current_price,
            previous_change,
            previous_change_rate,
        )
        .await;
        return Ok(result(
            rule,
            action,
            "matched",
            &format!(
                "{} 조건이 충족됐지만 추천 모드라 주문하지 않았습니다. AI: {} {:.0}% - {}",
                format_trigger(&rule.trigger),
                format_ai_action(&proposal.action),
                proposal.confidence * 100.0,
                proposal.reason
            ),
            Some(current_price),
            false,
            None,
            Some(&proposal),
        ));
    }

    let proposal = rule_ai_proposal(
        state,
        rule,
        current_price,
        previous_change,
        previous_change_rate,
    )
    .await;
    if let Some(block_reason) = ai_order_guard(state, action, &proposal) {
        return Ok(result(
            rule,
            action,
            "skipped",
            &block_reason,
            Some(current_price),
            false,
            None,
            Some(&proposal),
        ));
    }

    if let Some(block_reason) = auto_order_guard(state, rule, current_price, monitor_state)? {
        return Ok(result(
            rule,
            action,
            "skipped",
            &block_reason,
            Some(current_price),
            false,
            None,
            Some(&proposal),
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
        Some(&proposal),
    ))
}

async fn rule_ai_proposal(
    state: &AppState,
    rule: &TradingRule,
    current_price: u64,
    previous_change: Option<i64>,
    previous_change_rate: Option<f64>,
) -> ProposalResponse {
    strategy::proposal(
        state,
        &ProposalRequest {
            symbol: rule.symbol.clone(),
            name: Some(rule.name.clone()),
            current_price: Some(current_price),
            previous_change,
            previous_change_rate,
        },
    )
    .await
}

fn ai_order_guard(
    state: &AppState,
    expected_action: &str,
    proposal: &ProposalResponse,
) -> Option<String> {
    if proposal.action != expected_action {
        return Some(format!(
            "가격 조건은 충족됐지만 AI 판단이 {}라 {} 주문을 막았습니다. {}",
            format_ai_action(&proposal.action),
            format_ai_action(expected_action),
            proposal.reason
        ));
    }

    if proposal.confidence < state.config.auto_min_confidence {
        return Some(format!(
            "가격 조건은 충족됐지만 AI 신뢰도 {:.0}%가 기준 {:.0}%보다 낮아 주문하지 않았습니다. {}",
            proposal.confidence * 100.0,
            state.config.auto_min_confidence * 100.0,
            proposal.reason
        ));
    }

    None
}

fn result(
    rule: &TradingRule,
    action: &str,
    status: &str,
    reason: &str,
    current_price: Option<u64>,
    order_submitted: bool,
    cooldown_until_unix: Option<u64>,
    proposal: Option<&ProposalResponse>,
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
        ai_action: proposal.map(|proposal| proposal.action.clone()),
        ai_confidence: proposal.map(|proposal| proposal.confidence),
        ai_reason: proposal.map(|proposal| proposal.reason.clone()),
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

fn format_ai_action(action: &str) -> &'static str {
    match action {
        "buy" => "매수",
        "sell" => "매도",
        "hold" => "관망",
        _ => "판단 불가",
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

fn auto_order_guard(
    state: &AppState,
    rule: &TradingRule,
    current_price: u64,
    monitor_state: &RuleMonitorState,
) -> ApiResult<Option<String>> {
    let settings = risk_settings::get(state)?;
    let today = current_utc_day();
    if let Some(rule_state) = monitor_state.rules.get(&rule.id) {
        if rule_state.daily_execution_date == Some(today) && rule_state.daily_execution_count >= 1 {
            return Ok(Some(
                "이 규칙은 오늘 이미 자동주문을 실행해 추가 주문을 막았습니다.".to_string(),
            ));
        }
    }

    let amount = current_price * u64::from(rule.quantity);
    let used = monitor_state
        .symbols
        .get(&rule.symbol)
        .filter(|symbol_state| symbol_state.daily_order_date == today)
        .map(|symbol_state| symbol_state.daily_order_amount_krw)
        .unwrap_or(0);

    if used.saturating_add(amount) > settings.max_daily_auto_order_amount_krw_per_symbol {
        return Ok(Some(format!(
            "종목별 일일 자동주문 한도 {}원을 초과해 주문하지 않았습니다.",
            settings.max_daily_auto_order_amount_krw_per_symbol
        )));
    }

    Ok(None)
}

fn mark_rule_triggered(
    monitor_state: &mut RuleMonitorState,
    rule: &TradingRule,
    result: &RuleCheckResult,
    now: u64,
) {
    let today = current_utc_day();
    let rule_state = monitor_state
        .rules
        .entry(rule.id.clone())
        .or_insert(RuleState {
            last_triggered_at_unix: now,
            daily_execution_date: None,
            daily_execution_count: 0,
        });
    rule_state.last_triggered_at_unix = now;

    if result.order_submitted {
        if rule_state.daily_execution_date == Some(today) {
            rule_state.daily_execution_count += 1;
        } else {
            rule_state.daily_execution_date = Some(today);
            rule_state.daily_execution_count = 1;
        }

        let amount =
            result.current_price.unwrap_or(result.target_price) * u64::from(result.quantity);
        let symbol_state =
            monitor_state
                .symbols
                .entry(rule.symbol.clone())
                .or_insert(SymbolState {
                    daily_order_date: today,
                    daily_order_amount_krw: 0,
                });
        if symbol_state.daily_order_date == today {
            symbol_state.daily_order_amount_krw =
                symbol_state.daily_order_amount_krw.saturating_add(amount);
        } else {
            symbol_state.daily_order_date = today;
            symbol_state.daily_order_amount_krw = amount;
        }
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
    state
        .symbols
        .retain(|symbol, _| rules.iter().any(|rule| rule.symbol == *symbol));
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

fn current_utc_day() -> u64 {
    unix_now() / 86_400
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
