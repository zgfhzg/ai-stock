mod auto_trading;
mod config;
mod crypto;
mod error;
mod kis;
mod orders;
mod risk_settings;
mod routes;
mod rule_monitor;
mod state;
mod stocks;
mod strategy;
mod trading_rules;
mod watchlist;

use axum::{http::Method, Router};
use std::net::SocketAddr;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{config::AppConfig, routes::app_router, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|arg| arg == "--healthcheck") {
        return healthcheck().await;
    }

    config::load_dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = AppConfig::load();
    let port = config.api_port;
    let state = AppState::new(config);

    let app = Router::new()
        .merge(app_router())
        .with_state(state)
        .layer(cors())
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("api listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthcheck() -> anyhow::Result<()> {
    config::load_dotenv();
    let config = AppConfig::load();
    let url = format!("http://127.0.0.1:{}/health", config.api_port);
    let response = reqwest::get(url).await?;
    if response.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("healthcheck failed with {}", response.status())
    }
}

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_origin(Any)
        .allow_headers(Any)
}
