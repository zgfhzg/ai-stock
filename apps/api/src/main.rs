mod config;
mod error;
mod kis;
mod routes;
mod state;
mod strategy;
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

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any)
        .allow_headers(Any)
}
