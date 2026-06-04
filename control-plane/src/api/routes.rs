use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use crate::api::{handlers, AppState};

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/config", get(handlers::get_config))
        .route("/api/interfaces", get(handlers::get_interfaces))
        .route("/api/interfaces/:name/up", post(handlers::iface_up))
        .route("/api/interfaces/:name/down", post(handlers::iface_down))
        .route("/api/pppoe/clients", get(handlers::get_pppoe_clients))
        .route("/api/pppoe/status", get(handlers::get_pppoe_status))
        .route("/api/pppoe/create", post(handlers::create_pppoe_client))
        .route("/api/nat/status", get(handlers::get_nat_status))
        .route("/api/nat/enable", post(handlers::enable_nat))
        .route("/api/dhcp/status", get(handlers::get_dhcp_status))
        .route("/api/dhcp/enable", post(handlers::enable_dhcp))
        .route("/api/dns/status", get(handlers::get_dns_status))
        .route("/api/dns/enable", post(handlers::enable_dns))
        .route("/api/routes", get(handlers::get_routes))
}
