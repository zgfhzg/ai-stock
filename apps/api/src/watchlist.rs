use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct WatchlistItem {
    pub symbol: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct WatchlistItemInput {
    pub symbol: String,
    pub name: Option<String>,
}

pub fn list(state: &AppState) -> ApiResult<Vec<WatchlistItem>> {
    read_watchlist(&state.config.watchlist_path)
}

pub fn add(state: &AppState, input: WatchlistItemInput) -> ApiResult<Vec<WatchlistItem>> {
    let symbol = normalize_symbol(&input.symbol)?;
    let mut items = read_watchlist(&state.config.watchlist_path)?;

    if let Some(existing) = items.iter_mut().find(|item| item.symbol == symbol) {
        existing.name = input.name.unwrap_or_else(|| symbol.clone());
    } else {
        items.push(WatchlistItem {
            symbol: symbol.clone(),
            name: input.name.unwrap_or(symbol),
        });
    }

    items.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    write_watchlist(&state.config.watchlist_path, &items)?;
    Ok(items)
}

pub fn remove(state: &AppState, symbol: &str) -> ApiResult<Vec<WatchlistItem>> {
    let symbol = normalize_symbol(symbol)?;
    let mut items = read_watchlist(&state.config.watchlist_path)?;
    items.retain(|item| item.symbol != symbol);
    write_watchlist(&state.config.watchlist_path, &items)?;
    Ok(items)
}

fn read_watchlist(path: &str) -> ApiResult<Vec<WatchlistItem>> {
    if !Path::new(path).exists() {
        return Ok(default_watchlist());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("watchlist_read_failed", error))?;
    serde_json::from_str(&content).map_err(|error| file_error("watchlist_parse_failed", error))
}

fn write_watchlist(path: &str, items: &[WatchlistItem]) -> ApiResult<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| file_error("watchlist_dir_failed", error))?;
    }

    let content = serde_json::to_string_pretty(items)
        .map_err(|error| file_error("watchlist_serialize_failed", error))?;
    fs::write(path, format!("{}\n", content))
        .map_err(|error| file_error("watchlist_write_failed", error))
}

fn default_watchlist() -> Vec<WatchlistItem> {
    vec![WatchlistItem {
        symbol: "005930".to_string(),
        name: "삼성전자".to_string(),
    }]
}

fn normalize_symbol(symbol: &str) -> ApiResult<String> {
    let symbol = symbol.trim();
    if symbol.len() == 6 && symbol.chars().all(|char| char.is_ascii_digit()) {
        return Ok(symbol.to_string());
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            code: "invalid_symbol".to_string(),
            message: "Symbol must be a 6 digit Korean stock code.".to_string(),
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
