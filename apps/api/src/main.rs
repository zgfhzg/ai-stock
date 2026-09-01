use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    http: Client,
    token: Arc<RwLock<Option<CachedToken>>>,
}

#[derive(Clone, Serialize)]
struct AppConfig {
    env: String,
    trading_mode: String,
    live_trading_enabled: bool,
    strategy_url: String,
    kis_app_key: String,
    kis_app_secret: String,
    kis_account_no: String,
    kis_account_product_code: String,
    kis_base_url: String,
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
    kis: KisConfigStatus,
    strategy: StrategyHealth,
    risk: RiskConfig,
}

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Option<String>,
    fetched_at: Instant,
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

#[derive(Serialize)]
struct KisConfigStatus {
    configured: bool,
    base_url: String,
    account_configured: bool,
}

#[derive(Serialize)]
struct TokenStatus {
    status: String,
    expires_at: Option<String>,
    cached: bool,
}

#[derive(Serialize)]
struct KisApiResponse {
    rt_cd: Option<String>,
    msg_cd: Option<String>,
    msg1: Option<String>,
    output: Option<Value>,
    output1: Option<Value>,
    output2: Option<Value>,
}

#[derive(Serialize)]
struct ApiError {
    code: String,
    message: String,
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
        token: Arc::new(RwLock::new(None)),
    };

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/kis/config", get(kis_config))
        .route("/api/kis/token", post(kis_token))
        .route("/api/account/balance", get(account_balance))
        .route("/api/market/price/:symbol", get(market_price))
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
        kis: kis_config_status(&state.config),
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
    Json(kis_config_status(&state.config))
}

async fn kis_token(State(state): State<AppState>) -> ApiResult<Json<TokenStatus>> {
    ensure_kis_configured(&state.config)?;
    let token = get_access_token(&state).await?;

    Ok(Json(TokenStatus {
        status: "ok".to_string(),
        expires_at: token.expires_at,
        cached: token.cached,
    }))
}

async fn account_balance(State(state): State<AppState>) -> ApiResult<Json<KisApiResponse>> {
    ensure_kis_configured(&state.config)?;
    ensure_account_configured(&state.config)?;

    let tr_id = match state.config.trading_mode.as_str() {
        "live" | "real" => "TTTC8434R",
        _ => "VTTC8434R",
    };

    let params = [
        ("CANO", state.config.kis_account_no.as_str()),
        (
            "ACNT_PRDT_CD",
            state.config.kis_account_product_code.as_str(),
        ),
        ("AFHR_FLPR_YN", "N"),
        ("OFL_YN", ""),
        ("INQR_DVSN", "01"),
        ("UNPR_DVSN", "01"),
        ("FUND_STTL_ICLD_YN", "N"),
        ("FNCG_AMT_AUTO_RDPT_YN", "N"),
        ("PRCS_DVSN", "00"),
        ("CTX_AREA_FK100", ""),
        ("CTX_AREA_NK100", ""),
    ];

    let value = kis_get(
        &state,
        "/uapi/domestic-stock/v1/trading/inquire-balance",
        tr_id,
        &params,
    )
    .await?;

    Ok(Json(to_kis_response(value)))
}

async fn market_price(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> ApiResult<Json<KisApiResponse>> {
    ensure_kis_configured(&state.config)?;

    let params = [
        ("fid_cond_mrkt_div_code", "J"),
        ("fid_input_iscd", symbol.as_str()),
    ];

    let value = kis_get(
        &state,
        "/uapi/domestic-stock/v1/quotations/inquire-price",
        "FHKST01010100",
        &params,
    )
    .await?;

    Ok(Json(to_kis_response(value)))
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

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    access_token_token_expired: Option<String>,
}

struct AccessToken {
    value: String,
    expires_at: Option<String>,
    cached: bool,
}

async fn get_access_token(state: &AppState) -> ApiResult<AccessToken> {
    if let Some(token) = state.token.read().await.as_ref() {
        if token.fetched_at.elapsed() < Duration::from_secs(60 * 60 * 23) {
            return Ok(AccessToken {
                value: token.access_token.clone(),
                expires_at: token.expires_at.clone(),
                cached: true,
            });
        }
    }

    let url = format!("{}/oauth2/tokenP", state.config.kis_base_url);
    let body = serde_json::json!({
        "grant_type": "client_credentials",
        "appkey": state.config.kis_app_key,
        "appsecret": state.config.kis_app_secret,
    });

    let response = state
        .http
        .post(url)
        .header("content-type", "application/json; charset=UTF-8")
        .json(&body)
        .send()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_token_request_failed", error))?;

    let response = response
        .error_for_status()
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_token_http_error", error))?;

    let token = response
        .json::<TokenResponse>()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_token_parse_failed", error))?;

    let cached = CachedToken {
        access_token: token.access_token.clone(),
        expires_at: token.access_token_token_expired.clone(),
        fetched_at: Instant::now(),
    };
    *state.token.write().await = Some(cached);

    Ok(AccessToken {
        value: token.access_token,
        expires_at: token.access_token_token_expired,
        cached: false,
    })
}

async fn kis_get(
    state: &AppState,
    path: &str,
    tr_id: &str,
    params: &[(&str, &str)],
) -> ApiResult<Value> {
    let token = get_access_token(state).await?;
    let url = format!("{}{}", state.config.kis_base_url, path);
    let response = state
        .http
        .get(url)
        .header("content-type", "application/json; charset=UTF-8")
        .header("authorization", format!("Bearer {}", token.value))
        .header("appKey", state.config.kis_app_key.as_str())
        .header("appSecret", state.config.kis_app_secret.as_str())
        .header("tr_id", tr_id)
        .query(params)
        .send()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_request_failed", error))?;

    parse_kis_response(response).await
}

async fn parse_kis_response(response: Response) -> ApiResult<Value> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_response_read_failed", error))?;

    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: "kis_http_error".to_string(),
                message: text,
            }),
        ));
    }

    serde_json::from_str::<Value>(&text)
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_response_parse_failed", error))
}

fn to_kis_response(value: Value) -> KisApiResponse {
    KisApiResponse {
        rt_cd: value
            .get("rt_cd")
            .and_then(Value::as_str)
            .map(str::to_string),
        msg_cd: value
            .get("msg_cd")
            .and_then(Value::as_str)
            .map(str::to_string),
        msg1: value
            .get("msg1")
            .and_then(Value::as_str)
            .map(str::to_string),
        output: value.get("output").cloned(),
        output1: value.get("output1").cloned(),
        output2: value.get("output2").cloned(),
    }
}

fn ensure_kis_configured(config: &AppConfig) -> ApiResult<()> {
    if config.kis_app_key.is_empty() || config.kis_app_secret.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "kis_credentials_missing".to_string(),
                message: "KIS_APP_KEY and KIS_APP_SECRET must be configured.".to_string(),
            }),
        ));
    }

    Ok(())
}

fn ensure_account_configured(config: &AppConfig) -> ApiResult<()> {
    if config.kis_account_no.is_empty() || config.kis_account_product_code.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "kis_account_missing".to_string(),
                message: "KIS_ACCOUNT_NO and KIS_ACCOUNT_PRODUCT_CODE must be configured."
                    .to_string(),
            }),
        ));
    }

    Ok(())
}

fn api_error(
    status: StatusCode,
    code: impl Into<String>,
    error: impl std::fmt::Display,
) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            code: code.into(),
            message: error.to_string(),
        }),
    )
}

fn kis_config_status(config: &AppConfig) -> KisConfigStatus {
    KisConfigStatus {
        configured: !config.kis_app_key.is_empty() && !config.kis_app_secret.is_empty(),
        base_url: config.kis_base_url.clone(),
        account_configured: !config.kis_account_no.is_empty()
            && !config.kis_account_product_code.is_empty(),
    }
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
        kis_app_key: read_env("KIS_APP_KEY", ""),
        kis_app_secret: read_env("KIS_APP_SECRET", ""),
        kis_account_no: read_env("KIS_ACCOUNT_NO", ""),
        kis_account_product_code: read_env("KIS_ACCOUNT_PRODUCT_CODE", "01"),
        kis_base_url: read_env(
            "KIS_BASE_URL",
            "https://openapivts.koreainvestment.com:29443",
        ),
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
