use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Serialize;

use crate::{
    error::ApiResult,
    kis::{self, KisConfigStatus},
    orders::{self, OrderRequest, OrderResponse},
    state::AppState,
    strategy::{self, ProposalRequest, ProposalResponse, StrategyHealth},
    watchlist::{self, WatchlistItem, WatchlistItemInput},
};

pub fn app_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/kis/config", get(kis_config))
        .route("/api/kis/token", post(kis_token))
        .route("/api/account/balance", get(account_balance))
        .route("/api/market/price/:symbol", get(market_price))
        .route("/api/watchlist", get(watchlist).post(add_watchlist_item))
        .route("/api/watchlist/:symbol", delete(remove_watchlist_item))
        .route("/api/orders", get(order_logs).post(place_order))
        .route("/api/ai/proposal", post(proposal))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Serialize)]
struct SystemStatus {
    api: &'static str,
    trading_mode: String,
    live_trading_enabled: bool,
    kis: KisConfigStatus,
    strategy: StrategyHealth,
    risk: RiskConfig,
}

#[derive(Serialize)]
struct RiskConfig {
    max_order_amount_krw: u64,
    max_position_ratio: f64,
    daily_max_loss_ratio: f64,
    daily_max_order_count: u32,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "api",
    })
}

async fn status(State(state): State<AppState>) -> Json<SystemStatus> {
    let strategy = strategy::health(&state).await.unwrap_or(StrategyHealth {
        status: "unavailable".to_string(),
        service: "strategy".to_string(),
    });

    Json(SystemStatus {
        api: "ok",
        trading_mode: state.config.trading_mode.clone(),
        live_trading_enabled: state.config.live_trading_enabled,
        kis: kis::config_status(&state.config),
        strategy,
        risk: RiskConfig {
            max_order_amount_krw: state.config.max_order_amount_krw,
            max_position_ratio: state.config.max_position_ratio,
            daily_max_loss_ratio: state.config.daily_max_loss_ratio,
            daily_max_order_count: state.config.daily_max_order_count,
        },
    })
}

async fn kis_config(State(state): State<AppState>) -> Json<KisConfigStatus> {
    Json(kis::config_status(&state.config))
}

async fn kis_token(State(state): State<AppState>) -> ApiResult<Json<kis::TokenStatus>> {
    kis::ensure_kis_configured(&state.config)?;
    let token = kis::get_access_token(&state).await?;
    Ok(Json(kis::token_status(token)))
}

async fn account_balance(State(state): State<AppState>) -> ApiResult<Json<kis::KisApiResponse>> {
    Ok(Json(kis::get_balance(&state).await?))
}

async fn market_price(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> ApiResult<Json<kis::KisApiResponse>> {
    Ok(Json(kis::get_price(&state, &symbol).await?))
}

async fn watchlist(State(state): State<AppState>) -> ApiResult<Json<Vec<WatchlistItem>>> {
    Ok(Json(watchlist::list(&state)?))
}

async fn add_watchlist_item(
    State(state): State<AppState>,
    Json(input): Json<WatchlistItemInput>,
) -> ApiResult<Json<Vec<WatchlistItem>>> {
    Ok(Json(watchlist::add(&state, input)?))
}

async fn remove_watchlist_item(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> ApiResult<Json<Vec<WatchlistItem>>> {
    Ok(Json(watchlist::remove(&state, &symbol)?))
}

async fn order_logs(State(state): State<AppState>) -> ApiResult<Json<Vec<serde_json::Value>>> {
    Ok(Json(orders::list_logs(&state)?))
}

async fn place_order(
    State(state): State<AppState>,
    Json(request): Json<OrderRequest>,
) -> ApiResult<Json<OrderResponse>> {
    Ok(Json(orders::place(&state, request).await?))
}

async fn proposal(
    State(state): State<AppState>,
    Json(request): Json<ProposalRequest>,
) -> Json<ProposalResponse> {
    Json(strategy::proposal(&state, &request).await)
}
