use axum::{http::StatusCode, Json};
use reqwest::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    config::AppConfig,
    error::{api_error, ApiError, ApiResult},
    state::{AppState, CachedToken},
};

#[derive(Serialize)]
pub struct KisConfigStatus {
    pub configured: bool,
    pub base_url: String,
    pub account_configured: bool,
}

#[derive(Serialize)]
pub struct TokenStatus {
    pub status: String,
    pub expires_at: Option<String>,
    pub cached: bool,
}

#[derive(Serialize)]
pub struct KisApiResponse {
    pub rt_cd: Option<String>,
    pub msg_cd: Option<String>,
    pub msg1: Option<String>,
    pub output: Option<Value>,
    pub output1: Option<Value>,
    pub output2: Option<Value>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    access_token_token_expired: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct PersistedToken {
    access_token: String,
    expires_at: Option<String>,
    fetched_at_unix: u64,
}

pub struct AccessToken {
    pub value: String,
    pub expires_at: Option<String>,
    pub cached: bool,
}

pub async fn get_access_token(state: &AppState) -> ApiResult<AccessToken> {
    let mut token_guard = state.kis_token.write().await;

    if let Some(token) = token_guard.as_ref() {
        if token.fetched_at.elapsed() < Duration::from_secs(60 * 60 * 23) {
            return Ok(AccessToken {
                value: token.access_token.clone(),
                expires_at: token.expires_at.clone(),
                cached: true,
            });
        }
    }

    if let Some(token) = read_persisted_token(&state.config.kis_token_cache_path) {
        let access_token = token.access_token.clone();
        let expires_at = token.expires_at.clone();
        *token_guard = Some(token);

        return Ok(AccessToken {
            value: access_token,
            expires_at,
            cached: true,
        });
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

    let status = response.status();
    let token_text = response.text().await.map_err(|error| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "kis_token_response_read_failed",
            error,
        )
    })?;

    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: "kis_token_http_error".to_string(),
                message: token_text,
            }),
        ));
    }

    let token = serde_json::from_str::<TokenResponse>(&token_text)
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_token_parse_failed", error))?;

    let cached_token = CachedToken {
        access_token: token.access_token.clone(),
        expires_at: token.access_token_token_expired.clone(),
        fetched_at: Instant::now(),
    };
    write_persisted_token(&state.config.kis_token_cache_path, &cached_token);

    *token_guard = Some(cached_token);

    Ok(AccessToken {
        value: token.access_token,
        expires_at: token.access_token_token_expired,
        cached: false,
    })
}

fn read_persisted_token(path: &str) -> Option<CachedToken> {
    let content = fs::read_to_string(path).ok()?;
    let token = serde_json::from_str::<PersistedToken>(&content).ok()?;
    let now = unix_now()?;
    let age = now.checked_sub(token.fetched_at_unix)?;

    if age >= 60 * 60 * 23 {
        return None;
    }

    Some(CachedToken {
        access_token: token.access_token,
        expires_at: token.expires_at,
        fetched_at: Instant::now()
            .checked_sub(Duration::from_secs(age))
            .unwrap_or_else(Instant::now),
    })
}

fn write_persisted_token(path: &str, token: &CachedToken) {
    let Some(fetched_at_unix) = unix_now() else {
        return;
    };
    let persisted = PersistedToken {
        access_token: token.access_token.clone(),
        expires_at: token.expires_at.clone(),
        fetched_at_unix,
    };
    let Ok(content) = serde_json::to_string_pretty(&persisted) else {
        return;
    };

    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let _ = fs::write(path, content);
}

fn unix_now() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

pub async fn get_balance(state: &AppState) -> ApiResult<KisApiResponse> {
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
        state,
        "/uapi/domestic-stock/v1/trading/inquire-balance",
        tr_id,
        &params,
    )
    .await?;

    Ok(to_kis_response(value))
}

pub async fn get_price(state: &AppState, symbol: &str) -> ApiResult<KisApiResponse> {
    ensure_kis_configured(&state.config)?;

    let params = [("fid_cond_mrkt_div_code", "J"), ("fid_input_iscd", symbol)];

    let value = kis_get(
        state,
        "/uapi/domestic-stock/v1/quotations/inquire-price",
        "FHKST01010100",
        &params,
    )
    .await?;

    Ok(to_kis_response(value))
}

pub async fn place_cash_order(
    state: &AppState,
    side: &str,
    symbol: &str,
    quantity: u32,
    price: u64,
) -> ApiResult<KisApiResponse> {
    ensure_kis_configured(&state.config)?;
    ensure_account_configured(&state.config)?;

    let tr_id = match (state.config.trading_mode.as_str(), side) {
        ("live" | "real", "buy") => "TTTC0012U",
        ("live" | "real", "sell") => "TTTC0011U",
        (_, "buy") => "VTTC0012U",
        (_, "sell") => "VTTC0011U",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    code: "invalid_order_side".to_string(),
                    message: "Order side must be buy or sell.".to_string(),
                }),
            ));
        }
    };

    let body = serde_json::json!({
        "CANO": state.config.kis_account_no,
        "ACNT_PRDT_CD": state.config.kis_account_product_code,
        "PDNO": symbol,
        "ORD_DVSN": "00",
        "ORD_QTY": quantity.to_string(),
        "ORD_UNPR": price.to_string(),
        "EXCG_ID_DVSN_CD": "KRX",
        "SLL_TYPE": if side == "sell" { "01" } else { "" },
        "CNDT_PRIC": ""
    });

    let value = kis_post(
        state,
        "/uapi/domestic-stock/v1/trading/order-cash",
        tr_id,
        body,
    )
    .await?;

    Ok(to_kis_response(value))
}

pub fn token_status(token: AccessToken) -> TokenStatus {
    TokenStatus {
        status: "ok".to_string(),
        expires_at: token.expires_at,
        cached: token.cached,
    }
}

pub fn config_status(config: &AppConfig) -> KisConfigStatus {
    KisConfigStatus {
        configured: !config.kis_app_key.is_empty() && !config.kis_app_secret.is_empty(),
        base_url: config.kis_base_url.clone(),
        account_configured: !config.kis_account_no.is_empty()
            && !config.kis_account_product_code.is_empty(),
    }
}

pub fn ensure_kis_configured(config: &AppConfig) -> ApiResult<()> {
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

async fn kis_post(state: &AppState, path: &str, tr_id: &str, body: Value) -> ApiResult<Value> {
    let token = get_access_token(state).await?;
    let url = format!("{}{}", state.config.kis_base_url, path);
    let response = state
        .http
        .post(url)
        .header("content-type", "application/json; charset=UTF-8")
        .header("authorization", format!("Bearer {}", token.value))
        .header("appKey", state.config.kis_app_key.as_str())
        .header("appSecret", state.config.kis_app_secret.as_str())
        .header("tr_id", tr_id)
        .json(&body)
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

    let value = serde_json::from_str::<Value>(&text)
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "kis_response_parse_failed", error))?;

    if value.get("rt_cd").and_then(Value::as_str) == Some("1") {
        let message = value
            .get("msg1")
            .and_then(Value::as_str)
            .unwrap_or("KIS request failed.")
            .to_string();

        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                code: "kis_api_error".to_string(),
                message,
            }),
        ));
    }

    Ok(value)
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
