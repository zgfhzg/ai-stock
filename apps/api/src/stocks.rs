use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct Stock {
    pub symbol: String,
    pub name: String,
    pub market: String,
}

pub fn search(state: &AppState, query: &str) -> ApiResult<Vec<Stock>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let normalized_query = normalize(query);
    let mut matches = read_catalog(&state.config.stock_catalog_path)?
        .into_iter()
        .filter(|stock| {
            stock.symbol.contains(query) || normalize(&stock.name).contains(&normalized_query)
        })
        .collect::<Vec<_>>();

    matches.sort_by_key(|stock| rank_match(stock, query, &normalized_query));
    matches.truncate(10);
    Ok(matches)
}

pub fn resolve_one(state: &AppState, query: &str) -> ApiResult<Stock> {
    let query = query.trim();
    if query.len() == 6 && query.chars().all(|char| char.is_ascii_digit()) {
        if let Some(stock) = read_catalog(&state.config.stock_catalog_path)?
            .into_iter()
            .find(|stock| stock.symbol == query)
        {
            return Ok(stock);
        }
    }

    let normalized_query = normalize(query);
    let matches = search(state, query)?;

    if let Some(exact) = matches
        .iter()
        .find(|stock| normalize(&stock.name) == normalized_query)
    {
        return Ok(exact.clone());
    }

    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "stock_not_found".to_string(),
                message: "No matching stock was found. Try a listed stock name or 6 digit code."
                    .to_string(),
            }),
        )),
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "ambiguous_stock_name".to_string(),
                message: "Multiple stocks matched. Choose one from search results.".to_string(),
            }),
        )),
    }
}

pub fn resolve_news_stock(state: &AppState, name: &str, symbol: &str) -> ApiResult<Option<Stock>> {
    let name = name.trim();
    let symbol = symbol.trim();
    let catalog = read_catalog(&state.config.stock_catalog_path)?;

    if is_stock_symbol(symbol) {
        let Some(stock) = catalog.into_iter().find(|stock| stock.symbol == symbol) else {
            return Ok(None);
        };
        if name.is_empty() || normalize(&stock.name) == normalize(name) {
            return Ok(Some(stock));
        }
        return Ok(None);
    }

    if name.is_empty() {
        return Ok(None);
    }

    let normalized_name = normalize(name);
    let exact_matches = catalog
        .into_iter()
        .filter(|stock| normalize(&stock.name) == normalized_name)
        .collect::<Vec<_>>();

    match exact_matches.as_slice() {
        [stock] => Ok(Some(stock.clone())),
        _ => Ok(None),
    }
}

fn read_catalog(path: &str) -> ApiResult<Vec<Stock>> {
    if !Path::new(path).exists() {
        return Ok(default_catalog());
    }

    let content =
        fs::read_to_string(path).map_err(|error| file_error("stock_catalog_read_failed", error))?;
    serde_json::from_str(&content).map_err(|error| file_error("stock_catalog_parse_failed", error))
}

fn default_catalog() -> Vec<Stock> {
    vec![
        Stock {
            symbol: "005930".to_string(),
            name: "삼성전자".to_string(),
            market: "KOSPI".to_string(),
        },
        Stock {
            symbol: "000660".to_string(),
            name: "SK하이닉스".to_string(),
            market: "KOSPI".to_string(),
        },
    ]
}

fn is_stock_symbol(value: &str) -> bool {
    value.len() == 6 && value.chars().all(|char| char.is_ascii_digit())
}

fn rank_match(stock: &Stock, raw_query: &str, normalized_query: &str) -> u8 {
    let normalized_name = normalize(&stock.name);

    if stock.symbol == raw_query {
        0
    } else if normalized_name == normalized_query {
        1
    } else if normalized_name.starts_with(normalized_query) {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state_with_catalog(path: String) -> AppState {
        let mut config = crate::config::AppConfig::load();
        config.stock_catalog_path = path;
        AppState::new(config)
    }

    #[test]
    fn resolves_exact_news_stock_name() {
        let path = "../../data/stocks.json".to_string();
        let state = test_state_with_catalog(path);
        let stock = match resolve_news_stock(&state, "삼성전자", "") {
            Ok(Some(stock)) => stock,
            Ok(None) => panic!("expected 삼성전자 to resolve"),
            Err(_) => panic!("expected stock catalog lookup to succeed"),
        };
        assert_eq!(stock.symbol, "005930");
    }

    #[test]
    fn rejects_mismatched_news_stock_symbol() {
        let path = "../../data/stocks.json".to_string();
        let state = test_state_with_catalog(path);
        let stock = match resolve_news_stock(&state, "삼성전자우", "005930") {
            Ok(stock) => stock,
            Err(_) => panic!("expected stock catalog lookup to succeed"),
        };
        assert!(stock.is_none());
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|char| !char.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
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
