use axum::{http::StatusCode, Json};
use reqwest::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

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

    *token_guard = Some(CachedToken {
        access_token: token.access_token.clone(),
        expires_at: token.access_token_token_expired.clone(),
        fetched_at: Instant::now(),
    });

    Ok(AccessToken {
        value: token.access_token,
        expires_at: token.access_token_token_expired,
        cached: false,
    })
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
