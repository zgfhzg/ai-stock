use axum::{
    extract::State,
    http::Method,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr, sync::Arc};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    http: Client,
}

#[derive(Clone, Serialize)]
struct AppConfig {
    env: String,
    trading_mode: String,
    live_trading_enabled: bool,
    strategy_url: String,
    max_order_amount_krw: u64,
    max_position_ratio: f64,
    daily_max_loss_ratio: f64,
    daily_max_order_count: u32,
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
    strategy: StrategyHealth,
    risk: RiskConfig,
}

#[derive(Serialize, Deserialize)]
struct StrategyHealth {
    status: String,
    service: String,
}

#[derive(Serialize)]
struct RiskConfig {
    max_order_amount_krw: u64,
    max_position_ratio: f64,
    daily_max_loss_ratio: f64,
    daily_max_order_count: u32,
}

#[derive(Deserialize, Serialize)]
struct ProposalRequest {
    symbol: String,
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ProposalResponse {
    action: String,
    confidence: f64,
    reason: String,
    live_order_allowed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(load_config());
    let state = AppState {
        config: config.clone(),
        http: Client::new(),
    };

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/ai/proposal", post(proposal))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], read_env("API_PORT", "8080").parse()?));
    tracing::info!("api listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "api",
    })
}

async fn status(State(state): State<AppState>) -> Json<SystemStatus> {
    let strategy = strategy_health(&state).await.unwrap_or(StrategyHealth {
        status: "unavailable".to_string(),
        service: "strategy".to_string(),
    });

    Json(SystemStatus {
        api: "ok",
        trading_mode: state.config.trading_mode.clone(),
        live_trading_enabled: state.config.live_trading_enabled,
        strategy,
        risk: RiskConfig {
            max_order_amount_krw: state.config.max_order_amount_krw,
            max_position_ratio: state.config.max_position_ratio,
            daily_max_loss_ratio: state.config.daily_max_loss_ratio,
            daily_max_order_count: state.config.daily_max_order_count,
        },
    })
}

async fn proposal(
    State(state): State<AppState>,
    Json(request): Json<ProposalRequest>,
) -> Json<ProposalResponse> {
    let url = format!("{}/strategy/proposal", state.config.strategy_url);
    let response = state
        .http
        .post(url)
        .json(&request)
        .send()
        .await
        .and_then(|res| res.error_for_status());

    let mut proposal = match response {
        Ok(res) => res
            .json::<ProposalResponse>()
            .await
            .unwrap_or_else(|_| fallback_proposal(&request.symbol)),
        Err(_) => fallback_proposal(&request.symbol),
    };

    proposal.live_order_allowed = state.config.live_trading_enabled && proposal.action != "hold";
    Json(proposal)
}

async fn strategy_health(state: &AppState) -> anyhow::Result<StrategyHealth> {
    let url = format!("{}/health", state.config.strategy_url);
    let response = state.http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<StrategyHealth>().await?)
}

fn fallback_proposal(symbol: &str) -> ProposalResponse {
    ProposalResponse {
        action: "hold".to_string(),
        confidence: 0.0,
        reason: format!(
            "Strategy service unavailable. No order will be placed for {}.",
            symbol
        ),
        live_order_allowed: false,
    }
}

fn load_config() -> AppConfig {
    AppConfig {
        env: read_env("APP_ENV", "local"),
        trading_mode: read_env("TRADING_MODE", "paper"),
        live_trading_enabled: read_env("ENABLE_LIVE_TRADING", "false") == "true",
        strategy_url: read_env("STRATEGY_URL", "http://localhost:8090"),
        max_order_amount_krw: read_env("MAX_ORDER_AMOUNT_KRW", "100000")
            .parse()
            .unwrap_or(100000),
        max_position_ratio: read_env("MAX_POSITION_RATIO", "0.2").parse().unwrap_or(0.2),
        daily_max_loss_ratio: read_env("DAILY_MAX_LOSS_RATIO", "0.03")
            .parse()
            .unwrap_or(0.03),
        daily_max_order_count: read_env("DAILY_MAX_ORDER_COUNT", "20")
            .parse()
            .unwrap_or(20),
    }
}

fn read_env(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}
