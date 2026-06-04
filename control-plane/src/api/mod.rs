pub mod handlers;
pub mod openapi;
pub mod routes;
pub mod websocket;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use crate::config::Config;
use crate::services::manager::ServiceManager;
use crate::services::pppoe_auto::PppoeAutoConnectService;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

pub struct AppState {
    pub config: Config,
    pub service_manager: Arc<ServiceManager>,
    /// Broadcast channel for WebSocket real-time updates
    pub ws_tx: broadcast::Sender<websocket::WsMessage>,
    /// PPPoE auto-connect service
    pub pppoe_auto: Arc<PppoeAutoConnectService>,
}

pub async fn start_server(listen: &str, config: Config, service_manager: Arc<ServiceManager>) -> anyhow::Result<()> {
    // Create broadcast channel for WebSocket
    let (ws_tx, _) = broadcast::channel(100);

    // Create PPPoE auto-connect service from config
    let auto_config = config.network.pppoe.as_ref()
        .and_then(|p| p.autoconnect.as_ref())
        .cloned()
        .unwrap_or_default();
    let pppoe_auto = Arc::new(PppoeAutoConnectService::new(auto_config));

    let state = Arc::new(AppState { config, service_manager, ws_tx: ws_tx.clone(), pppoe_auto });

    // Start the stats broadcaster background task
    websocket::start_stats_broadcaster(ws_tx);

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
