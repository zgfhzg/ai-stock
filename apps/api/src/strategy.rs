use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Serialize, Deserialize)]
pub struct StrategyHealth {
    pub status: String,
    pub service: String,
}

#[derive(Deserialize, Serialize)]
pub struct ProposalRequest {
    pub symbol: String,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ProposalResponse {
    pub action: String,
    pub confidence: f64,
    pub reason: String,
    pub live_order_allowed: bool,
}

pub async fn health(state: &AppState) -> anyhow::Result<StrategyHealth> {
    let url = format!("{}/health", state.config.strategy_url);
    let response = state.http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<StrategyHealth>().await?)
}

pub async fn proposal(state: &AppState, request: &ProposalRequest) -> ProposalResponse {
    let url = format!("{}/strategy/proposal", state.config.strategy_url);
    let response = state
        .http
        .post(url)
        .json(request)
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
    proposal
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
