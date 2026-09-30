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
    pub current_price: Option<u64>,
    pub previous_change: Option<i64>,
    pub previous_change_rate: Option<f64>,
    pub holding_quantity: Option<u32>,
    pub average_purchase_price: Option<u64>,
    pub evaluation_profit_rate: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct ProposalResponse {
    pub action: String,
    pub confidence: f64,
    pub reason: String,
    pub live_order_allowed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewsArticleInput {
    pub id: i64,
    pub title: String,
    pub source: String,
    pub summary: Option<String>,
}

#[derive(Serialize)]
pub struct NewsAnalysisRequest {
    pub articles: Vec<NewsArticleInput>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RelatedStock {
    pub name: String,
    pub symbol: String,
    pub relevance: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewsAnalysis {
    pub article_id: i64,
    pub summary: String,
    pub sentiment: String,
    pub sentiment_score: f64,
    pub importance: u8,
    pub impact_horizon: String,
    pub related_stocks: Vec<RelatedStock>,
    pub rationale: String,
}

#[derive(Deserialize)]
pub struct NewsAnalysisResponse {
    pub model: String,
    pub analyses: Vec<NewsAnalysis>,
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

pub async fn analyze_news(
    state: &AppState,
    articles: Vec<NewsArticleInput>,
) -> anyhow::Result<NewsAnalysisResponse> {
    let url = format!("{}/news/analyze", state.config.strategy_url);
    let response = state
        .http
        .post(url)
        .json(&NewsAnalysisRequest { articles })
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<NewsAnalysisResponse>().await?)
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
