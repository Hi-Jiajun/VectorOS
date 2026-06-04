use axum::Router;
use axum::routing::{get, post, delete};
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
        .route("/api/interfaces/:name/config", post(handlers::configure_interface))
        .route("/api/interfaces/:name/stats", get(handlers::get_interface_stats))
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
        .route("/api/system/vpp-performance", get(handlers::get_vpp_performance))
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
        // QoS management
        .route("/api/qos/status", get(handlers::get_qos_status))
        .route("/api/qos/policer", post(handlers::create_policer))
        .route("/api/qos/policer/:name", delete(handlers::delete_policer))
        .route("/api/qos/interface/:name/limit", post(handlers::set_interface_rate_limit))
        // Flow monitoring
        .route("/api/flows/status", get(handlers::get_flow_status))
        .route("/api/flows/top", get(handlers::get_flow_top))
        .route("/api/flows/export", post(handlers::set_flow_export))
        .route("/api/flows/export/enable", post(handlers::enable_flow_export))
        .route("/api/flows/export/disable", post(handlers::disable_flow_export))
        .route("/api/flows/classify-setup", post(handlers::setup_flow_classify))
        .route("/api/flows/list", get(handlers::list_flows))
        // Connection tracking
        .route("/api/conntrack/status", get(handlers::get_conntrack_status))
        .route("/api/conntrack/connections", get(handlers::get_conntrack_connections))
        .route("/api/conntrack/stats", get(handlers::get_conntrack_stats))
        .route("/api/conntrack/top", get(handlers::get_conntrack_top))
        .route("/api/conntrack/filter", post(handlers::filter_conntrack_connections))
        .route("/api/conntrack/detail", get(handlers::get_conntrack_detail))
        // Traffic control
        .route("/api/traffic/status", get(handlers::get_traffic_status))
        .route("/api/traffic/limit", post(handlers::set_traffic_limit))
        .route("/api/traffic/limit/interface/:iface", delete(handlers::remove_traffic_interface_limit))
        .route("/api/traffic/limit/ip/:ip", delete(handlers::remove_traffic_ip_limit))
        .route("/api/traffic/priority", post(handlers::set_traffic_priority))
        .route("/api/traffic/app-class", post(handlers::set_traffic_app_class))
        .route("/api/traffic/app-class/:name", delete(handlers::remove_traffic_app_class))
        .route("/api/traffic/defaults", post(handlers::load_traffic_defaults))
        .route("/api/traffic/stats", get(handlers::get_traffic_stats))
        .route("/api/traffic/reset", post(handlers::reset_traffic))
        // VPN management
        .route("/api/vpn/status", get(handlers::get_vpn_status))
        .route("/api/vpn/connections", get(handlers::get_vpn_connections))
        .route("/api/vpn/wireguard/config", post(handlers::configure_wireguard))
        .route("/api/vpn/ipsec/config", post(handlers::configure_ipsec))
        .route("/api/vpn/openvpn/config", post(handlers::configure_openvpn))
        .route("/api/vpn/down", post(handlers::vpn_down))
}
