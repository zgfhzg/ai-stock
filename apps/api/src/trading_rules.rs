use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
    stocks,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct TradingRule {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub trigger: RuleTrigger,
    pub target_price: u64,
    pub quantity: u32,
    pub enabled: bool,
    pub created_at_unix: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTrigger {
    BuyBelow,
    SellAbove,
    StopLoss,
    TakeProfit,
}

#[derive(Deserialize)]
pub struct TradingRuleInput {
    pub query: Option<String>,
    pub symbol: Option<String>,
    pub trigger: RuleTrigger,
    pub target_price: u64,
    pub quantity: u32,
    pub enabled: Option<bool>,
}

pub fn list(state: &AppState) -> ApiResult<Vec<TradingRule>> {
    read_rules(&state.config.auto_rules_path)
}

pub fn add(state: &AppState, input: TradingRuleInput) -> ApiResult<Vec<TradingRule>> {
    validate_rule_values(input.target_price, input.quantity)?;

    let stock = match input.query.as_deref() {
        Some(query) if !query.trim().is_empty() => stocks::resolve_one(state, query)?,
        _ => {
            let symbol = input.symbol.as_deref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: "missing_rule_stock".to_string(),
                        message: "Stock name or 6 digit code is required.".to_string(),
                    }),
                )
            })?;
            stocks::resolve_one(state, symbol)?
        }
    };

    let now = unix_now();
    let mut rules = read_rules(&state.config.auto_rules_path)?;
    rules.push(TradingRule {
        id: format!("rule-{now}"),
        symbol: stock.symbol,
        name: stock.name,
        trigger: input.trigger,
        target_price: input.target_price,
        quantity: input.quantity,
        enabled: input.enabled.unwrap_or(true),
        created_at_unix: now,
    });

    rules.sort_by(|left, right| {
        left.symbol
            .cmp(&right.symbol)
            .then(left.created_at_unix.cmp(&right.created_at_unix))
    });
    write_rules(&state.config.auto_rules_path, &rules)?;
    Ok(rules)
}

pub fn remove(state: &AppState, id: &str) -> ApiResult<Vec<TradingRule>> {
    let mut rules = read_rules(&state.config.auto_rules_path)?;
    rules.retain(|rule| rule.id != id);
    write_rules(&state.config.auto_rules_path, &rules)?;
    Ok(rules)
}

fn validate_rule_values(target_price: u64, quantity: u32) -> ApiResult<()> {
    if target_price == 0 {
        return validation_error("invalid_rule_price", "Rule target price must be positive.");
    }

    if quantity == 0 {
        return validation_error("invalid_rule_quantity", "Rule quantity must be positive.");
    }

    Ok(())
}

fn read_rules(path: &str) -> ApiResult<Vec<TradingRule>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("rules_read_failed", error))?;
    serde_json::from_str(&content).map_err(|error| file_error("rules_parse_failed", error))
}

fn write_rules(path: &str, rules: &[TradingRule]) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("rules_dir_failed", error))?;
    }

    let content = serde_json::to_string_pretty(rules)
        .map_err(|error| file_error("rules_serialize_failed", error))?;
    fs::write(path, format!("{content}\n")).map_err(|error| file_error("rules_write_failed", error))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn validation_error<T>(code: impl Into<String>, message: impl Into<String>) -> ApiResult<T> {
    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            code: code.into(),
            message: message.into(),
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
