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
