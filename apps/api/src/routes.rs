use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auto_trading::{self, AutoRunRequest, AutoRunResponse},
    crypto::{
        self, CryptoConfigStatus, CryptoInstrument, CryptoOrderRequest, CryptoOrderResponse,
        CryptoQuote,
    },
    error::ApiResult,
    kis::{self, KisConfigStatus},
    orders::{self, OrderRequest, OrderResponse},
    state::AppState,
    stocks::{self, Stock},
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
        .route("/api/stocks/search", get(search_stocks))
        .route("/api/crypto/config", get(crypto_config))
        .route(
            "/api/crypto/instruments/:market_type",
            get(crypto_instruments),
        )
        .route("/api/crypto/quote/:market_type/:symbol", get(crypto_quote))
        .route(
            "/api/crypto/orders",
            get(crypto_order_logs).post(place_crypto_order),
        )
        .route("/api/watchlist", get(watchlist).post(add_watchlist_item))
        .route("/api/watchlist/:symbol", delete(remove_watchlist_item))
        .route("/api/orders", get(order_logs).post(place_order))
        .route("/api/auto-trading/run", post(run_auto_trading))
        .route("/api/auto-trading/runs", get(auto_trading_logs))
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
    crypto: CryptoConfigStatus,
    strategy: StrategyHealth,
    risk: RiskConfig,
}

#[derive(Serialize)]
struct RiskConfig {
    max_order_amount_krw: u64,
    max_position_ratio: f64,
    daily_max_loss_ratio: f64,
    daily_max_order_count: u32,
    max_crypto_order_amount_usdt: f64,
}

#[derive(Deserialize)]
struct StockSearchQuery {
    q: String,
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
        crypto: crypto::config_status(&state),
        strategy,
        risk: RiskConfig {
            max_order_amount_krw: state.config.max_order_amount_krw,
            max_position_ratio: state.config.max_position_ratio,
            daily_max_loss_ratio: state.config.daily_max_loss_ratio,
            daily_max_order_count: state.config.daily_max_order_count,
            max_crypto_order_amount_usdt: state.config.max_crypto_order_amount_usdt,
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

async fn search_stocks(
    State(state): State<AppState>,
    Query(query): Query<StockSearchQuery>,
) -> ApiResult<Json<Vec<Stock>>> {
    Ok(Json(stocks::search(&state, &query.q)?))
}

async fn crypto_config(State(state): State<AppState>) -> Json<CryptoConfigStatus> {
    Json(crypto::config_status(&state))
}

async fn crypto_instruments(
    Path(market_type): Path<String>,
) -> ApiResult<Json<Vec<CryptoInstrument>>> {
    Ok(Json(crypto::instruments(&market_type)?))
}

async fn crypto_quote(
    State(state): State<AppState>,
    Path((market_type, symbol)): Path<(String, String)>,
) -> ApiResult<Json<CryptoQuote>> {
    Ok(Json(crypto::quote(&state, &market_type, &symbol).await?))
}

async fn crypto_order_logs(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    Ok(Json(crypto::list_order_logs(&state)?))
}

async fn place_crypto_order(
    State(state): State<AppState>,
    Json(request): Json<CryptoOrderRequest>,
) -> ApiResult<Json<CryptoOrderResponse>> {
    Ok(Json(crypto::place_order(&state, request).await?))
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

async fn run_auto_trading(
    State(state): State<AppState>,
    Json(request): Json<AutoRunRequest>,
) -> ApiResult<Json<AutoRunResponse>> {
    Ok(Json(auto_trading::run_once(&state, request).await?))
}

async fn auto_trading_logs(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    Ok(Json(auto_trading::list_logs(&state)?))
}

async fn proposal(
    State(state): State<AppState>,
    Json(request): Json<ProposalRequest>,
) -> Json<ProposalResponse> {
    Json(strategy::proposal(&state, &request).await)
}
