use utoipa::{openapi, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

use crate::api::handlers;
use crate::auth::{LoginRequest, LoginResponse};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "VectorOS API",
        version = "0.1.0",
        description = "VectorOS - VPP-based Open Source Router Management API",
        contact(name = "VectorOS", url = "https://github.com/vectoros"),
        license(name = "Apache-2.0", url = "https://www.apache.org/licenses/LICENSE-2.0")
    ),
    paths(
        handlers::health,
        handlers::login,
        handlers::get_config,
        handlers::get_interfaces,
        handlers::iface_up,
        handlers::iface_down,
        handlers::configure_interface,
        handlers::get_interface_stats,
        handlers::get_pppoe_clients,
        handlers::get_pppoe_status,
        handlers::create_pppoe_client,
        handlers::get_nat_status,
        handlers::enable_nat,
        handlers::get_dhcp_status,
        handlers::enable_dhcp,
        handlers::get_dns_status,
        handlers::enable_dns,
        handlers::get_routes,
        handlers::get_system_status,
        handlers::get_vpp_performance,
        handlers::get_config_status,
        handlers::save_config,
        // FRRouting
        handlers::get_frr_status,
        handlers::get_frr_routes,
        handlers::add_frr_route,
        handlers::del_frr_route,
        // IPv6
        handlers::get_ipv6_status,
        handlers::get_ipv6_neighbors,
        handlers::get_dhcpv6_status,
        // Logs
        handlers::get_logs,
        handlers::clear_logs,
        // Firewall
        handlers::get_firewall_status,
        handlers::add_firewall_rule,
        handlers::update_firewall_rule,
        handlers::delete_firewall_rule,
        handlers::reorder_firewall_rules,
        handlers::enable_firewall,
        handlers::disable_firewall,
        // Firewall Groups
        handlers::list_firewall_groups,
        handlers::add_firewall_group,
        handlers::delete_firewall_group,
        handlers::add_rule_to_group,
        handlers::remove_rule_from_group,
        // Firewall Aliases
        handlers::list_firewall_aliases,
        handlers::add_firewall_alias,
        handlers::update_firewall_alias,
        handlers::delete_firewall_alias,
        handlers::refresh_firewall_alias,
        // Firewall Schedules
        handlers::list_firewall_schedules,
        handlers::add_firewall_schedule,
        handlers::delete_firewall_schedule,
        // GeoIP
        handlers::update_firewall_geoip,
        // Traffic Shaper
        handlers::get_shaper_status,
        handlers::set_shaper_interface,
        handlers::remove_shaper_interface,
        handlers::add_shaper_queue,
        handlers::delete_shaper_queue,
        // IDS
        handlers::update_ids_config,
        handlers::get_ids_alerts,
        handlers::clear_ids_alerts,
        handlers::get_ids_stats,
        // QoS
        handlers::get_qos_status,
        handlers::create_policer,
        handlers::delete_policer,
        handlers::set_interface_rate_limit,
        // Flows
        handlers::get_flow_status,
        handlers::get_flow_top,
        handlers::set_flow_export,
        handlers::enable_flow_export,
        handlers::disable_flow_export,
        handlers::setup_flow_classify,
        handlers::list_flows,
        // ConnTrack
        handlers::get_conntrack_status,
        handlers::get_conntrack_connections,
        handlers::get_conntrack_stats,
        handlers::get_conntrack_top,
        handlers::filter_conntrack_connections,
        handlers::get_conntrack_detail,
        // Traffic Control
        handlers::get_traffic_status,
        handlers::set_traffic_limit,
        handlers::remove_traffic_interface_limit,
        handlers::remove_traffic_ip_limit,
        handlers::set_traffic_priority,
        handlers::set_traffic_app_class,
        handlers::remove_traffic_app_class,
        handlers::load_traffic_defaults,
        handlers::get_traffic_stats,
        handlers::reset_traffic,
        // VPN
        handlers::get_vpn_status,
        handlers::get_vpn_connections,
        handlers::configure_wireguard,
        handlers::configure_ipsec,
        handlers::configure_openvpn,
        handlers::vpn_down,
        // Diagnostics
        handlers::get_diag_status,
        handlers::diag_ping,
        handlers::diag_traceroute,
        handlers::diag_dns,
        handlers::diag_portscan,
        // Configuration Management
        handlers::get_config_tree,
        handlers::get_config_staging,
        handlers::config_set_value,
        handlers::config_delete_value,
        handlers::config_commit,
        handlers::config_rollback,
        handlers::config_discard,
        handlers::config_diff,
        handlers::config_diff_versions,
        handlers::config_history,
        handlers::config_list_templates,
        handlers::config_save_template,
        handlers::config_apply_template,
        handlers::config_cli_session,
        handlers::config_cli_execute,
        handlers::config_export,
        handlers::config_import,
        handlers::config_validate,
        handlers::config_import_history,
        // Services
        handlers::list_services,
        handlers::get_service_status,
        handlers::start_service,
        handlers::stop_service,
        handlers::restart_service,
        handlers::reload_service,
    ),
    components(schemas(
        LoginRequest,
        LoginResponse,
        handlers::PppoeConfig,
        handlers::InterfaceConfigRequest,
        handlers::AddRouteRequest,
        handlers::DelRouteRequest,
        handlers::LogQuery,
        handlers::FirewallRuleRequest,
        handlers::FirewallRuleDelete,
        handlers::FirewallRuleUpdate,
        handlers::ReorderRulesReq,
        handlers::AddGroupReq,
        handlers::GroupRuleReq,
        handlers::AddAliasReq,
        handlers::UpdateAliasReq,
        handlers::AddScheduleReq,
        handlers::GeoIpReq,
        handlers::ShaperIfaceReq,
        handlers::ShaperQueueReq,
        handlers::IdsConfigReq,
        handlers::CreatePolicerReq,
        handlers::SetInterfaceLimitReq,
        handlers::FlowExportSetRequest,
        handlers::ConntrackFilterRequest,
        handlers::TrafficLimitRequest,
        handlers::TrafficIpLimitRequest,
        handlers::TrafficPriorityRequest,
        handlers::TrafficAppClassRequest,
        handlers::TrafficIpDeleteRequest,
        handlers::TrafficLimitBody,
        handlers::VpnDownRequest,
        handlers::ConfigSetRequest,
        handlers::ConfigDeleteRequest,
        handlers::SaveTemplateRequest,
        handlers::ApplyTemplateRequest,
        handlers::CliSessionRequest,
        handlers::CliExecuteRequest,
        handlers::ConfigExportRequest,
        handlers::ConfigImportRequest,
        handlers::ConfigValidateRequest,
        crate::services::vpn::WireGuardConfigRequest,
        crate::services::vpn::IpsecConfigRequest,
        crate::services::vpn::OpenVpnConfigRequest,
        crate::services::diag::PingRequest,
        crate::services::diag::TracerouteRequest,
        crate::services::diag::DnsRequest,
        crate::services::diag::PortScanRequest,
        crate::services::firewall::TimeRange,
    )),
    tags(
        (name = "System", description = "System health and status"),
        (name = "Authentication", description = "Login and token management"),
        (name = "Interfaces", description = "Network interface management"),
        (name = "PPPoE", description = "PPPoE client management"),
        (name = "NAT", description = "NAT configuration"),
        (name = "DHCP", description = "DHCP server management"),
        (name = "DNS", description = "DNS resolver management"),
        (name = "FRRouting", description = "FRRouting BGP/OSPF configuration"),
        (name = "IPv6", description = "IPv6 and DHCPv6 management"),
        (name = "Firewall", description = "Firewall rules, groups, aliases, and schedules"),
        (name = "GeoIP", description = "GeoIP-based filtering"),
        (name = "Traffic Shaper", description = "Traffic shaping and QoS"),
        (name = "IDS", description = "Intrusion Detection System (Suricata)"),
        (name = "QoS", description = "Quality of Service policers and rate limits"),
        (name = "Flows", description = "Flow monitoring and export"),
        (name = "ConnTrack", description = "Connection tracking"),
        (name = "Traffic Control", description = "Bandwidth limits, priorities, and app classification"),
        (name = "VPN", description = "VPN tunnel management (WireGuard, IPsec, OpenVPN)"),
        (name = "Diagnostics", description = "Network diagnostic tools (ping, traceroute, DNS, port scan)"),
        (name = "Logs", description = "Log management"),
        (name = "Configuration", description = "VyOS-style hierarchical configuration management"),
        (name = "Routes", description = "Routing table management"),
        (name = "Services", description = "System service management (start, stop, restart, reload)"),
    ),
    modifiers(&SecurityAddon),
    security(("bearer_auth" = []))
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                openapi::security::SecurityScheme::Http(
                    openapi::security::Http::new(openapi::security::HttpAuthScheme::Bearer),
                ),
            );
        }
    }
}

pub fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui/{*path}")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
}
