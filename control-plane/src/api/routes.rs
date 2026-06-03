use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use crate::api::{handlers, AppState};

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/config", get(handlers::get_config))
        .route("/api/interfaces", get(handlers::get_interfaces))
        .route("/api/routes", get(handlers::get_routes))
        .route("/api/dhcp/leases", get(handlers::get_dhcp_leases))
}
