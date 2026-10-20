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
    ai_plans::{self, StoredPlan},
    error::{ApiError, ApiResult},
    kis,
    orders::{self, OrderRequest, OrderResponse},
    state::AppState,
    strategy::ProposalResponse,
    watchlist::{self, WatchlistItem},
};

#[derive(Deserialize)]
pub struct AutoRunRequest {
    pub execute: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AutoRunResponse {
    pub mode: String,
    pub universe_mode: String,
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
    #[serde(default)]
    pub price_plan: Option<StoredPlan>,
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
    pub universe_mode: String,
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
    pub fn new(interval_seconds: u64, universe_mode: String) -> Self {
        Self {
            handle: None,
            status: AutoMonitorStatus {
                running: false,
                universe_mode,
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
    let _run_guard = state.auto_run_lock.lock().await;
    let (holdings, account_equity) = load_holdings(state).await?;
    let mut items = holdings
        .iter()
        .filter(|(_, holding)| holding.quantity > 0)
        .map(|(symbol, _)| WatchlistItem {
            symbol: symbol.clone(),
            name: crate::stocks::resolve_one(state, symbol)
                .map(|stock| stock.name)
                .unwrap_or_else(|_| symbol.clone()),
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    let stored_plans = ai_plans::list(state).map_err(|e| file_error("ai_plan_read_failed", e))?;
    for record in &stored_plans {
        if record.submitted_side.is_some()
            && !items.iter().any(|item| item.symbol == record.plan.symbol)
        {
            items.push(WatchlistItem {
                symbol: record.plan.symbol.clone(),
                name: record.plan.symbol.clone(),
            });
        }
    }
    let candidates = match auto_universe(state).await {
        Ok(items) => items,
        Err((_, Json(error))) if !items.is_empty() => {
            tracing::warn!(
                "AI candidates unavailable; continuing holdings: {}",
                error.message
            );
            Vec::new()
        }
        Err(error) => return Err(error),
    };
    for item in candidates {
        if !items.iter().any(|existing| existing.symbol == item.symbol) {
            items.push(item);
        }
    }
    let execute = request.execute.unwrap_or(false)
        && state.config.auto_trade_mode == "paper_auto"
        && state.config.trading_mode == "paper";
    let risk = crate::risk_settings::get(state)?;
    let portfolio_value = holdings
        .values()
        .map(|holding| holding.current_value)
        .sum::<u64>();
    let mut remaining_budget = risk.auto_trading_budget_krw.saturating_sub(portfolio_value);
    let pending_buy_amount = stored_plans
        .iter()
        .filter(|record| record.submitted_side.as_deref() == Some("buy"))
        .map(|record| {
            let (price, quantity) = if let Some(order) = &record.broker_order {
                let reflected = holdings
                    .get(&record.plan.symbol)
                    .map(|holding| holding.quantity)
                    .unwrap_or(0)
                    .saturating_sub(record.holding_before_order);
                (
                    order.price,
                    order
                        .remaining_quantity
                        .saturating_add(order.filled_quantity.saturating_sub(reflected)),
                )
            } else {
                (record.plan.entry_price, record.submitted_quantity)
            };
            price.saturating_mul(u64::from(quantity))
        })
        .sum::<u64>();
    remaining_budget = remaining_budget.saturating_sub(pending_buy_amount);
    let daily_loss_blocked = match account_equity {
        Some(equity) => {
            ai_plans::daily_loss_blocked(state, equity, unix_now(), risk.daily_max_loss_ratio)
                .map_err(|error| file_error("account_loss_check_failed", error))?
        }
        None => true,
    };
    if daily_loss_blocked {
        remaining_budget = 0;
    }
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
        universe_mode: state.config.auto_universe_mode.clone(),
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
    if state.config.auto_universe_mode == "watchlist" && watchlist::list(state)?.is_empty() {
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

async fn auto_universe(state: &AppState) -> ApiResult<Vec<WatchlistItem>> {
    if state.config.auto_universe_mode == "watchlist" {
        return watchlist::list(state);
    }

    let mut outlooks = crate::news::daily_outlooks(state)?;
    if outlooks.is_empty() {
        outlooks = match crate::news::generate_daily_outlooks(state).await {
            Ok(outlooks) => outlooks,
            Err((_, Json(error))) if error.code == "no_daily_news_events" => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
    }
    outlooks.retain(|outlook| outlook.signal_expires_at_unix > unix_now() as i64);
    outlooks.sort_by(|left, right| {
        right
            .confidence
            .total_cmp(&left.confidence)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });

    Ok(outlooks
        .into_iter()
        .take(state.config.auto_market_candidate_limit)
        .map(|candidate| WatchlistItem {
            symbol: candidate.symbol,
            name: candidate.name,
        })
        .collect())
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
        let pending_orders = ai_plans::list(&state)
            .map(|plans| plans.iter().any(|plan| plan.submitted_side.is_some()))
            .unwrap_or(true);
        let wait_seconds = if pending_orders {
            interval_seconds.min(30)
        } else {
            interval_seconds
        };
        state.auto_monitor.lock().await.status.next_check_at_unix = Some(unix_now() + wait_seconds);
        sleep(Duration::from_secs(wait_seconds)).await;
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
    build_decision_at(
        state,
        item,
        holding,
        execute,
        risk,
        remaining_budget,
        unix_now(),
    )
    .await
}

async fn build_decision_at(
    state: &AppState,
    item: &WatchlistItem,
    holding: Option<&HoldingInfo>,
    execute: bool,
    risk: &crate::risk_settings::RiskSettings,
    remaining_budget: &mut u64,
    now: u64,
) -> ApiResult<AutoDecision> {
    let held = holding.map(|value| value.quantity).unwrap_or(0);
    let mut record = ai_plans::load(state, &item.symbol)
        .map_err(|error| file_error("ai_plan_read_failed", error))?;
    if let Some(pending) = record
        .as_mut()
        .filter(|record| record.submitted_side.is_some())
    {
        let outcome = crate::broker_orders::manage(
            state,
            pending,
            (held, holding.map(|value| value.current_value).unwrap_or(0)),
            execute,
            risk,
            *remaining_budget,
            now,
        )
        .await;
        let reason = match outcome {
            Ok(()) => pending
                .broker_order
                .as_ref()
                .map(|order| order.reason.clone())
                .unwrap_or_else(|| pending.order_status.clone()),
            Err(error) => format!("증권사 주문 확인 대기: {error}"),
        };
        let mut decision = proposal_to_decision(
            item,
            &ProposalResponse {
                action: "hold".into(),
                confidence: pending.plan.confidence,
                reason,
                live_order_allowed: false,
            },
            None,
            None,
            None,
        );
        decision.skip_reason = Some("order_reconciliation_pending".into());
        decision.price_plan = record;
        return Ok(decision);
    }
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
                price_plan: None,
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
    let auction_quote = if ai_plans::opening_auction(now) {
        kis::get_auction_quote(state, &item.symbol).await.ok()
    } else {
        None
    };
    let outlook = crate::news::daily_outlooks(state)?
        .into_iter()
        .find(|outlook| outlook.symbol == item.symbol);
    let context = serde_json::json!({
        "symbol": item.symbol,
        "name": item.name,
        "current_price": current_price.unwrap_or(0),
        "quote": output,
        "auction_quote": auction_quote.as_ref().and_then(kis::auction_output),
        "news": outlook,
        "holding_quantity": held,
        "average_purchase_price": holding.map(|h| h.average_price).unwrap_or(0),
        "evaluation_profit_rate": holding.and_then(|h| h.profit_rate),
        "maximum_buy_amount_krw": (*remaining_budget).min(risk.max_order_amount_krw)
            .min(risk.max_daily_auto_order_amount_krw_per_symbol)
            .min((risk.auto_trading_budget_krw as f64 * risk.max_position_ratio) as u64),
        "maximum_order_amount_krw": risk.max_order_amount_krw,
        "previous_plan": null,
    });
    let mut decision = proposal_to_decision(
        item,
        &ProposalResponse {
            action: "hold".to_string(),
            confidence: 0.0,
            reason: "AI 가격 계획 대기".to_string(),
            live_order_allowed: false,
        },
        current_price,
        previous_change,
        previous_change_rate,
    );
    let Some(price) = current_price.filter(|value| *value > 0) else {
        decision.skip_reason = Some("quote_unavailable".to_string());
        return Ok(decision);
    };
    if let Some(reason) = crate::news::instrument_block_reason(output) {
        decision.reason = reason;
        decision.skip_reason = Some("instrument_restricted".to_string());
        return Ok(decision);
    }
    let needs_plan = record.as_ref().is_none_or(|existing| {
        let exit_due = held > 0 && ai_plans::trigger(&existing.plan, price, held).is_some();
        let news_changed = held == 0
            && existing.context["news"]["generated_at_unix"]
                != context["news"]["generated_at_unix"];
        !exit_due && (existing.expires_at_unix <= now || news_changed)
    });
    if needs_plan {
        let settings = crate::news_analysis_settings::get(state)?;
        let kst = now + 9 * 3600;
        let minute = kst % 86_400 / 60;
        let start = u64::from(settings.collection_hour_kst) * 60
            + u64::from(settings.collection_minute_kst);
        if (kst / 86_400 + 3) % 7 >= 5 || minute < start || minute >= 15 * 60 + 20 {
            decision.reason =
                "장전 준비 시각부터 정규장 감시 시간에 AI 계획을 생성합니다.".to_string();
            decision.skip_reason = Some("waiting_plan_session".to_string());
            decision.price_plan = record;
            return Ok(decision);
        }
        let mut context = context;
        context["previous_plan"] = record
            .as_ref()
            .map(|stored| serde_json::to_value(&stored.plan).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        match ai_plans::generate(state, context, now).await {
            Ok(generated) => record = Some(generated),
            Err(error) if held == 0 || record.is_none() => {
                decision.reason = format!("AI 가격 계획 생성 보류: {error}");
                decision.skip_reason = Some("ai_plan_unavailable".to_string());
                return Ok(decision);
            }
            Err(error) => {
                tracing::warn!("AI replan unavailable; retaining protective exit: {error}")
            }
        }
    }
    let mut record = record.expect("generated or persisted plan");
    let price = if ai_plans::opening_auction(now) && record.plan.session == "opening_auction" {
        let indicative = auction_quote
            .as_ref()
            .and_then(kis::auction_reference_price);
        let Some(indicative) = indicative else {
            decision.skip_reason = Some("auction_quote_unavailable".into());
            decision.reason = "장전 예상체결가 확인 대기 중입니다.".into();
            decision.price_plan = Some(record);
            return Ok(decision);
        };
        decision.current_price = Some(indicative);
        indicative
    } else {
        price
    };
    if record.order_status == "rejected" && now < record.expires_at_unix {
        decision.reason = "증권사 주문 거절 후 재평가 대기 중입니다.".to_string();
        decision.skip_reason = Some("order_rejected".to_string());
        decision.price_plan = Some(record);
        return Ok(decision);
    }
    let triggered = ai_plans::trigger(&record.plan, price, held);
    let proposal = ProposalResponse {
        action: triggered.unwrap_or("hold").to_string(),
        confidence: record.plan.confidence,
        reason: record.plan.reason.clone(),
        live_order_allowed: false,
    };
    decision.action = proposal.action.clone();
    decision.confidence = proposal.confidence;
    decision.reason = proposal.reason.clone();
    let order_price = if proposal.action == "buy"
        || (proposal.action == "sell"
            && record.plan.action == "sell"
            && price >= record.plan.entry_price)
    {
        record.plan.entry_price
    } else {
        price
    };
    let mut quantity = calculate_quantity(
        &proposal,
        Some(order_price),
        holding,
        risk,
        *remaining_budget,
    )
    .min(record.plan.quantity.max(
        if proposal.action == "sell" && record.plan.action == "hold" {
            held
        } else {
            0
        },
    ));
    if proposal.action == "buy" {
        let used = today_buy_amount(state, &item.symbol, now)?;
        let room = risk
            .max_daily_auto_order_amount_krw_per_symbol
            .saturating_sub(used);
        quantity = quantity.min(u32::try_from(room / order_price).unwrap_or(u32::MAX));
    }
    decision.quantity = quantity;
    decision.order_amount_krw = order_price.saturating_mul(u64::from(quantity));
    let protective_exit = held > 0
        && proposal.action == "sell"
        && ((record.plan.stop_loss_price > 0 && price <= record.plan.stop_loss_price)
            || (record.plan.take_profit_price > 0 && price >= record.plan.take_profit_price));

    if execute
        && ai_plans::order_session_allowed(&record.plan, now)
        && (protective_exit || should_submit_order(state, &proposal, current_price))
        && quantity > 0
    {
        record.submitted_side = Some(proposal.action.clone());
        record.submitted_quantity = quantity;
        record.holding_before_order = held;
        record.order_status = "submitting".to_string();
        record.broker_order = Some(
            crate::broker_orders::pending(&proposal.action, order_price, quantity, now)
                .map_err(|error| file_error("broker_order_init_failed", error))?,
        );
        // Persist before contacting the broker: an uncertain response must not cause a duplicate.
        ai_plans::record_order_event(state, &record, "submit_intent", now)
            .map_err(|e| file_error("ai_plan_save_failed", e))?;
        let order = match submit_order(state, &proposal, item, order_price, quantity).await {
            Ok(order) => order,
            Err((_, Json(error))) => {
                let rejected = matches!(
                    error.code.as_str(),
                    "kis_api_error"
                        | "max_order_amount_exceeded"
                        | "daily_order_count_exceeded"
                        | "invalid_quantity"
                        | "invalid_price"
                        | "invalid_symbol"
                );
                if rejected {
                    record.submitted_side = None;
                    record.order_status = "rejected".to_string();
                    record.expires_at_unix = now + 15 * 60;
                    ai_plans::save(state, &record)
                        .map_err(|e| file_error("ai_plan_save_failed", e))?;
                }
                decision.reason = format!("주문 처리 보류: {}", error.message);
                decision.skip_reason = Some(
                    if rejected {
                        "order_rejected"
                    } else {
                        "order_reconciliation_pending"
                    }
                    .to_string(),
                );
                decision.price_plan = Some(record);
                return Ok(decision);
            }
        };
        decision.order_submitted = order.accepted;
        if let Some(tracked) = record.broker_order.as_mut() {
            crate::broker_orders::capture_identity(tracked, order.kis.output.as_ref());
            tracked.reason = "증권사 주문 접수, 체결 확인 대기".into();
        }
        record.order_status = if order.accepted {
            "accepted"
        } else {
            "rejected"
        }
        .to_string();
        if order.accepted && proposal.action == "buy" {
            *remaining_budget = remaining_budget.saturating_sub(order.order_amount_krw);
        }
        if !order.accepted {
            record.submitted_side = None;
            record.expires_at_unix = now + 15 * 60;
            decision.skip_reason = Some("order_rejected".to_string());
        }
        ai_plans::record_order_event(state, &record, "submit_result", now)
            .map_err(|e| file_error("ai_plan_save_failed", e))?;
    } else if proposal.action != "hold" {
        decision.skip_reason = Some(
            if execute {
                if !ai_plans::order_session_allowed(&record.plan, now) {
                    "waiting_regular_session"
                } else if quantity == 0 {
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
    if triggered.is_none() {
        decision.skip_reason = Some("waiting_ai_price".to_string());
    }
    decision.price_plan = Some(record);
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
        price_plan: None,
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

pub(crate) fn today_buy_amount(state: &AppState, symbol: &str, now: u64) -> ApiResult<u64> {
    let path = &state.config.order_log_path;
    let day_start = ((now + 9 * 3600) / 86_400) * 86_400 - 9 * 3600;
    let content = if Path::new(path).exists() {
        fs::read_to_string(path).map_err(|error| file_error("order_log_read_failed", error))?
    } else {
        String::new()
    };
    let mut amount = 0_u64;
    let mut identified = HashMap::new();
    for line in content.lines() {
        let entry: Value = serde_json::from_str(line)
            .map_err(|error| file_error("order_log_parse_failed", error))?;
        if entry["timestamp_unix"]
            .as_u64()
            .is_some_and(|time| time >= day_start)
            && entry["response"]["accepted"].as_bool() == Some(true)
            && entry["request"]["side"].as_str() == Some("buy")
            && entry["request"]["symbol"].as_str() == Some(symbol)
        {
            let gross = entry["response"]["order_amount_krw"]
                .as_u64()
                .unwrap_or(u64::MAX);
            let order_no = entry["response"]["kis"]["output"]["ODNO"]
                .as_str()
                .or_else(|| entry["response"]["kis"]["output"]["odno"].as_str());
            if let Some(order_no) = order_no {
                let date: String = rusqlite::Connection::open_in_memory()
                    .and_then(|db| {
                        db.query_row(
                            "SELECT strftime('%Y%m%d', ?1, 'unixepoch', '+9 hours')",
                            [entry["timestamp_unix"].as_u64().unwrap_or(now)],
                            |row| row.get(0),
                        )
                    })
                    .map_err(|error| file_error("order_date_failed", error))?;
                identified.insert(format!("{date}:{order_no}"), gross);
            } else {
                amount = amount.saturating_add(gross);
            }
        }
    }
    crate::broker_orders::update_buy_commitments(state, symbol, day_start, &mut identified)
        .map_err(|error| file_error("order_commitments_failed", error))?;
    Ok(identified
        .values()
        .fold(amount, |sum, value| sum.saturating_add(*value)))
}

async fn load_holdings(state: &AppState) -> ApiResult<(HashMap<String, HoldingInfo>, Option<u64>)> {
    let balance = kis::get_balance(state).await?;
    let account_equity = balance
        .output2
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row["tot_evlu_amt"].as_str())
        .and_then(|value| value.replace(',', "").parse::<u64>().ok())
        .filter(|value| *value > 0);
    let rows = balance
        .output1
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let holdings = rows
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
                    average_price: row
                        .get("pchs_avg_pric")
                        .and_then(Value::as_str)
                        .and_then(|value| value.replace(',', "").parse::<f64>().ok())
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .unwrap_or(0.0) as u64,
                    current_value: parse_u64("evlu_amt"),
                    profit_rate: row
                        .get("evlu_pfls_rt")
                        .and_then(Value::as_str)
                        .and_then(|value| value.parse::<f64>().ok()),
                },
            ))
        })
        .collect();
    Ok((holdings, account_equity))
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

    #[tokio::test]
    async fn persisted_plan_drives_buy_wait_fill_and_protective_sell() {
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        };
        let price = Arc::new(AtomicU64::new(69_000));
        let quote_price = price.clone();
        let app = axum::Router::new().route(
            "/uapi/domestic-stock/v1/quotations/inquire-price",
            axum::routing::get(move || {
                let price = quote_price.load(Ordering::Relaxed);
                async move {
                    Json(serde_json::json!({"rt_cd":"0", "output": {
                        "stck_prpr": price.to_string(), "acml_vol":"0",
                        "iscd_stat_cls_code":"00", "mrkt_warn_cls_code":"00", "temp_stop_yn":"N"
                    }}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock listener");
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let directory = std::env::temp_dir().join(format!(
            "ai-price-flow-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut config = crate::config::AppConfig::load();
        config.news_db_path = directory.join("news.db").to_string_lossy().into_owned();
        config.order_log_path = directory
            .join("orders.jsonl")
            .to_string_lossy()
            .into_owned();
        config.kis_base_url = format!("http://{address}");
        config.kis_app_key = "test".into();
        config.kis_app_secret = "test".into();
        let state = AppState::new(config);
        crate::news::initialize(&state).unwrap();
        ai_plans::initialize(&state).unwrap();
        *state.kis_token.write().await = Some(crate::state::CachedToken {
            access_token: "mock".into(),
            expires_at: None,
            fetched_at: std::time::Instant::now(),
        });
        let mut record = StoredPlan {
            plan: ai_plans::PricePlan {
                symbol: "005930".into(),
                action: "buy".into(),
                confidence: 0.8,
                entry_price: 70_000,
                take_profit_price: 75_000,
                stop_loss_price: 65_000,
                quantity: 1,
                valid_for_minutes: 60,
                reason: "AI news price plan".into(),
                session: "regular".into(),
            },
            model: "mock".into(),
            generated_at_unix: unix_now(),
            expires_at_unix: unix_now() + 3600,
            context: serde_json::json!({"news":null}),
            submitted_side: None,
            submitted_quantity: 0,
            holding_before_order: 0,
            order_status: "watching".into(),
            broker_order: None,
        };
        ai_plans::save(&state, &record).unwrap();
        let item = WatchlistItem {
            symbol: "005930".into(),
            name: "test".into(),
        };
        let mut budget = 500_000;
        let buy = build_decision(&state, &item, None, false, &risk(), &mut budget)
            .await
            .unwrap_or_else(|_| panic!("buy decision"));
        assert_eq!(buy.action, "buy");
        assert_eq!(buy.quantity, 1);
        assert!(!buy.order_submitted);
        record.submitted_side = Some("buy".into());
        record.submitted_quantity = 1;
        ai_plans::save(&state, &record).unwrap();
        let pending = build_decision(&state, &item, None, false, &risk(), &mut budget)
            .await
            .unwrap_or_else(|_| panic!("pending decision"));
        assert_eq!(
            pending.skip_reason.as_deref(),
            Some("order_reconciliation_pending")
        );
        let holding = HoldingInfo {
            quantity: 1,
            average_price: 70_000,
            current_value: 64_000,
            profit_rate: Some(-8.5),
        };
        price.store(64_000, Ordering::Relaxed);
        // A legacy order without a broker identity is never inferred filled from balance alone.
        record.submitted_side = None;
        record.order_status = "filled".into();
        ai_plans::save(&state, &record).unwrap();
        let sell = build_decision(&state, &item, Some(&holding), false, &risk(), &mut budget)
            .await
            .unwrap_or_else(|_| panic!("sell decision"));
        assert_eq!(sell.action, "sell");
        assert_eq!(sell.quantity, 1);
        assert_eq!(sell.price_plan.unwrap().order_status, "filled");
        server.abort();
        let _ = server.await;
        fs::remove_dir_all(directory).unwrap();
    }

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

    #[tokio::test]
    async fn opening_auction_submits_ai_limit_once_and_requires_indicative_quote() {
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        };
        let indicative = Arc::new(AtomicU64::new(69_000));
        let feed = indicative.clone();
        let submitted = Arc::new(Mutex::new(Vec::<Value>::new()));
        let capture = submitted.clone();
        let app = axum::Router::new()
            .route("/uapi/domestic-stock/v1/quotations/inquire-price",axum::routing::get(|| async {
                Json(serde_json::json!({"rt_cd":"0","output":{"stck_prpr":"71000","acml_vol":"0","iscd_stat_cls_code":"00","mrkt_warn_cls_code":"00","temp_stop_yn":"N"}}))
            }))
            .route("/uapi/domestic-stock/v1/quotations/inquire-asking-price-exp-ccn",axum::routing::get(move || {
                let price = feed.load(Ordering::Relaxed);
                async move {Json(serde_json::json!({"rt_cd":"0","output2":{"antc_cnpr":price.to_string()}}))}
            }))
            .route("/uapi/domestic-stock/v1/trading/order-cash",axum::routing::post(move |headers:axum::http::HeaderMap,Json(body):Json<Value>| {
                capture.lock().unwrap().push(body.clone());
                async move {
                    assert_eq!(headers["tr_id"],"VTTC0012U");
                    assert_eq!(body["ORD_DVSN"],"00");
                    assert_eq!(body["ORD_UNPR"],"70000");
                    Json(serde_json::json!({"rt_cd":"0","output":{"ODNO":"123","KRX_FWDG_ORD_ORGNO":"06010"}}))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let directory = std::env::temp_dir().join(format!(
            "auction-submit-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut config = crate::config::AppConfig::load();
        config.news_db_path = directory.join("news.db").to_string_lossy().into_owned();
        config.order_log_path = directory
            .join("orders.jsonl")
            .to_string_lossy()
            .into_owned();
        config.risk_settings_path = directory.join("risk.json").to_string_lossy().into_owned();
        config.kis_base_url = format!("http://{address}");
        config.kis_app_key = "test".into();
        config.kis_app_secret = "test".into();
        config.kis_account_no = "12345678".into();
        config.trading_mode = "paper".into();
        config.max_order_amount_krw = 100_000;
        config.daily_max_order_count = 20;
        config.auto_min_confidence = 0.7;
        let state = AppState::new(config);
        crate::news::initialize(&state).unwrap();
        ai_plans::initialize(&state).unwrap();
        *state.kis_token.write().await = Some(crate::state::CachedToken {
            access_token: "mock".into(),
            expires_at: None,
            fetched_at: std::time::Instant::now(),
        });
        let now = 20_000 * 86_400 - 9 * 3600 + 8 * 3600 + 45 * 60;
        let mut record = StoredPlan {
            plan: ai_plans::PricePlan {
                symbol: "005930".into(),
                action: "buy".into(),
                confidence: 0.9,
                entry_price: 70_000,
                take_profit_price: 75_000,
                stop_loss_price: 65_000,
                quantity: 1,
                valid_for_minutes: 60,
                reason: "auction test".into(),
                session: "opening_auction".into(),
            },
            model: "mock".into(),
            generated_at_unix: now,
            expires_at_unix: now + 3600,
            context: serde_json::json!({"news":null}),
            submitted_side: None,
            submitted_quantity: 0,
            holding_before_order: 0,
            order_status: "watching".into(),
            broker_order: None,
        };
        ai_plans::save(&state, &record).unwrap();
        let item = WatchlistItem {
            symbol: "005930".into(),
            name: "test".into(),
        };
        let mut budget = 500_000;
        let decision = build_decision_at(&state, &item, None, true, &risk(), &mut budget, now)
            .await
            .unwrap_or_else(|_| panic!("auction submit"));
        assert!(decision.order_submitted);
        assert_eq!(decision.current_price, Some(69_000));
        assert_eq!(submitted.lock().unwrap().len(), 1);
        let _ = build_decision_at(&state, &item, None, true, &risk(), &mut budget, now + 1).await;
        assert_eq!(submitted.lock().unwrap().len(), 1);
        indicative.store(0, Ordering::Relaxed);
        record.plan.symbol = "000660".into();
        ai_plans::save(&state, &record).unwrap();
        let item = WatchlistItem {
            symbol: "000660".into(),
            name: "test".into(),
        };
        let decision = build_decision_at(&state, &item, None, true, &risk(), &mut budget, now)
            .await
            .unwrap_or_else(|_| panic!("missing indicative quote"));
        assert_eq!(
            decision.skip_reason.as_deref(),
            Some("auction_quote_unavailable")
        );
        assert_eq!(submitted.lock().unwrap().len(), 1);
        server.abort();
        let _ = server.await;
        fs::remove_dir_all(directory).unwrap();
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
