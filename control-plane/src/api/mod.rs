pub mod handlers;
pub mod routes;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use crate::config::Config;
use std::sync::Arc;
use tracing::info;

pub struct AppState {
    pub config: Config,
}

pub async fn start_server(listen: &str, config: Config) -> anyhow::Result<()> {
    let state = Arc::new(AppState { config });

    let app = Router::new()
        .merge(routes::api_routes())
        .nest_service("/", ServeDir::new("frontend/dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    info!("API server listening on {}", listen);
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
