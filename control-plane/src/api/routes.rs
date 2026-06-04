use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use crate::api::{handlers, AppState};

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Public routes (no auth required)
        .route("/api/health", get(handlers::health))
        .route("/api/auth/login", post(handlers::login))
        // Protected routes
        .route("/api/config", get(handlers::get_config))
        .route("/api/config/save", post(handlers::save_config))
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
        .route("/api/system", get(handlers::get_system_status))
        .route("/api/config/status", get(handlers::get_config_status))
        // FRRouting
        .route("/api/frr/status", get(handlers::get_frr_status))
        .route("/api/frr/routes", get(handlers::get_frr_routes))
        .route("/api/frr/add-route", post(handlers::add_frr_route))
        .route("/api/frr/del-route", post(handlers::del_frr_route))
        // IPv6
        .route("/api/ipv6/status", get(handlers::get_ipv6_status))
        .route("/api/ipv6/neighbors", get(handlers::get_ipv6_neighbors))
        .route("/api/dhcpv6/status", get(handlers::get_dhcpv6_status))
        // Log management
        .route("/api/logs", post(handlers::get_logs))
        .route("/api/logs/clear", post(handlers::clear_logs))
        // Firewall management
        .route("/api/firewall/status", get(handlers::get_firewall_status))
        .route("/api/firewall/add-rule", post(handlers::add_firewall_rule))
        .route("/api/firewall/del-rule", post(handlers::delete_firewall_rule))
        .route("/api/firewall/enable", post(handlers::enable_firewall))
        .route("/api/firewall/disable", post(handlers::disable_firewall))
}
