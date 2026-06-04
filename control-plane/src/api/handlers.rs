use axum::extract::{State, Path};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;
use crate::api::AppState;
use crate::auth::{LoginRequest, LoginResponse};

#[derive(Debug, Deserialize)]
pub struct PppoeConfig {
    pub username: String,
    pub password: String,
    pub interface: String,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
    #[serde(default = "default_mru")]
    pub mru: u32,
    #[serde(default)]
    pub use_peer_dns: bool,
    #[serde(default)]
    pub add_default_route4: bool,
    #[serde(default)]
    pub add_default_route6: bool,
}

fn default_mtu() -> u32 { 1492 }
fn default_mru() -> u32 { 1492 }

/// Run a Python VPP command
fn run_vpp_cmd(action: &str, args: &[(&str, &str)]) -> Result<Value, String> {
    let mut cmd = Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/pppoe_manager.py");
    cmd.arg(action);

    for (key, value) in args {
        cmd.arg(format!("--{}", key));
        cmd.arg(value);
    }

    let output = cmd.output().map_err(|e| format!("Failed to run command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse output: {}", e))
}

pub async fn login(Json(req): Json<LoginRequest>) -> Json<Value> {
    if crate::auth::verify_credentials(&req.username, &req.password) {
        match crate::auth::generate_token(&req.username) {
            Ok(token) => Json(json!({
                "success": true,
                "data": {
                    "token": token,
                    "expires_in": 86400
                }
            })),
            Err(e) => Json(json!({
                "success": false,
                "error": { "code": "TOKEN_ERROR", "message": e.to_string() }
            })),
        }
    } else {
        Json(json!({
            "success": false,
            "error": { "code": "INVALID_CREDENTIALS", "message": "Invalid username or password" }
        }))
    }
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "config": state.config }))
}

pub async fn get_interfaces() -> Json<Value> {
    match crate::vpp::native::get_interfaces() {
        Ok(interfaces) => Json(json!({ "interfaces": interfaces })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn iface_up(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::set_interface_state(&name, "up") {
        Ok(()) => Json(json!({ "status": "ok", "message": format!("Interface {} set to up", name) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn iface_down(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::set_interface_state(&name, "down") {
        Ok(()) => Json(json!({ "status": "ok", "message": format!("Interface {} set to down", name) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct InterfaceConfigRequest {
    pub mtu: Option<u32>,
    pub ip_add: Option<String>,
    pub ip_remove: Option<String>,
    pub promiscuous: Option<bool>,
}

/// POST /api/interfaces/:name/config
/// Configure interface: MTU, IP add/remove, promiscuous mode.
pub async fn configure_interface(
    Path(name): Path<String>,
    Json(req): Json<InterfaceConfigRequest>,
) -> Json<Value> {
    let mut errors: Vec<String> = Vec::new();
    let mut applied: Vec<String> = Vec::new();

    if let Some(mtu) = req.mtu {
        match crate::vpp::native::set_interface_mtu(&name, mtu) {
            Ok(()) => applied.push(format!("mtu set to {}", mtu)),
            Err(e) => errors.push(format!("mtu: {}", e)),
        }
    }

    if let Some(ip) = req.ip_add {
        match crate::vpp::native::set_interface_ip(&name, &ip) {
            Ok(()) => applied.push(format!("IP {} added", ip)),
            Err(e) => errors.push(format!("ip add {}: {}", ip, e)),
        }
    }

    if let Some(ip) = req.ip_remove {
        match crate::vpp::native::remove_interface_ip(&name, &ip) {
            Ok(()) => applied.push(format!("IP {} removed", ip)),
            Err(e) => errors.push(format!("ip remove {}: {}", ip, e)),
        }
    }

    if let Some(promisc) = req.promiscuous {
        let result = if promisc {
            crate::vpp::native::enable_interface_promisc(&name)
        } else {
            crate::vpp::native::disable_interface_promisc(&name)
        };
        match result {
            Ok(()) => applied.push(format!("promiscuous mode {}", if promisc { "enabled" } else { "disabled" })),
            Err(e) => errors.push(format!("promiscuous: {}", e)),
        }
    }

    if errors.is_empty() {
        Json(json!({ "status": "ok", "applied": applied }))
    } else {
        Json(json!({ "status": if applied.is_empty() { "error" } else { "partial" }, "applied": applied, "errors": errors }))
    }
}

/// GET /api/interfaces/:name/stats
/// Get detailed interface statistics (packets, bytes, errors, drops).
pub async fn get_interface_stats(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::get_interface_stats(&name) {
        Ok(stats) => Json(json!({ "stats": stats })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_pppoe_clients() -> Json<Value> {
    match run_vpp_cmd("dump", &[]) {
        Ok(data) => Json(json!({ "clients": data })),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn get_pppoe_status() -> Json<Value> {
    match crate::vpp::native::get_pppoe_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn create_pppoe_client(
    Json(config): Json<PppoeConfig>,
) -> Json<Value> {
    // Map interface name to sw_if_index
    let sw_if_index = match config.interface.as_str() {
        "enp1s0" => "1",
        "enp2s0" => "2",
        "enp3s0" => "3",
        _ => return Json(json!({ "error": format!("Unknown interface: {}", config.interface) })),
    };

    let mtu_str = config.mtu.to_string();
    let mru_str = config.mru.to_string();

    let args = vec![
        ("sw-if-index", sw_if_index),
        ("username", &config.username),
        ("password", &config.password),
        ("mtu", &mtu_str),
        ("mru", &mru_str),
    ];

    match run_vpp_cmd("create", &args) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn get_nat_status() -> Json<Value> {
    match crate::vpp::native::get_nat_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn enable_nat() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/nat_manager.py");
    cmd.arg("enable");
    cmd.arg("--inside-if").arg("2");
    cmd.arg("--outside-if").arg("4");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

pub async fn get_system_status() -> Json<Value> {
    let system_info = crate::vpp::native::get_system_info();
    let vpp_perf = crate::vpp::native::get_vpp_performance();

    match system_info {
        Ok(info) => {
            let mut response = json!({
                "system": {
                    "cpu": {
                        "percent": info.cpu_percent,
                        "count": info.cpu_count
                    },
                    "memory": {
                        "total": info.memory_total,
                        "used": info.memory_used,
                        "percent": info.memory_percent
                    },
                    "disk": {
                        "total": info.disk_total,
                        "used": info.disk_used,
                        "percent": info.disk_percent
                    }
                },
                "vpp": {
                    "version": info.vpp_version,
                    "interface_count": info.interface_count
                }
            });

            // Merge VPP performance metrics when available
            if let Ok(perf) = vpp_perf {
                if let Some(obj) = response.as_object_mut() {
                    obj.insert("performance".to_string(), json!({
                        "packet_rate": perf.packet_rate,
                        "interfaces": perf.interfaces,
                        "nat": perf.nat,
                        "pppoe": perf.pppoe,
                        "memory": perf.memory,
                        "threads": perf.threads,
                        "errors": perf.errors,
                    }));
                }
            }

            Json(response)
        }
        Err(e) => {
            // Even if system info fails, try to return VPP performance alone
            match vpp_perf {
                Ok(perf) => Json(json!({
                    "system": { "error": e.to_string() },
                    "vpp": { "performance": {
                        "packet_rate": perf.packet_rate,
                        "interfaces": perf.interfaces,
                        "nat": perf.nat,
                        "pppoe": perf.pppoe,
                        "memory": perf.memory,
                        "threads": perf.threads,
                        "errors": perf.errors,
                    }}
                })),
                Err(e2) => Json(json!({
                    "error": e.to_string(),
                    "performance_error": e2.to_string()
                })),
            }
        }
    }
}

pub async fn get_vpp_performance() -> Json<Value> {
    match crate::vpp::native::get_vpp_performance() {
        Ok(perf) => Json(json!({
            "performance": {
                "packet_rate": perf.packet_rate,
                "interfaces": perf.interfaces,
                "nat": perf.nat,
                "pppoe": perf.pppoe,
                "memory": {
                    "total_mb": perf.memory.total,
                    "used_mb": perf.memory.used,
                    "free_mb": perf.memory.free,
                    "percent": perf.memory.percent,
                },
                "threads": perf.threads,
                "errors": perf.errors,
            }
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_config_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/config_manager.py");
    cmd.arg("get");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn save_config(Json(config): Json<Value>) -> Json<Value> {
    let config_str = serde_json::to_string(&config).unwrap_or_default();

    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/config_manager.py");
    cmd.arg("set");
    cmd.arg("--section").arg("all");
    cmd.arg("--key").arg("config");
    cmd.arg("--value").arg(&config_str);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

// ── DNS handlers (native Rust) ──────────────────────────────────────

pub async fn get_dns_status() -> Json<Value> {
    match crate::services::dns::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn enable_dns() -> Json<Value> {
    match crate::services::dns::enable(crate::services::dns::DnsEnableConfig::default()) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── FRRouting handlers (native Rust) ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddRouteRequest {
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub distance: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DelRouteRequest {
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub distance: Option<u32>,
}

pub async fn get_frr_status() -> Json<Value> {
    match crate::services::frr::get_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn get_frr_routes() -> Json<Value> {
    match crate::services::frr::show_routes() {
        Ok(routes) => Json(json!({ "routes": routes })),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn add_frr_route(Json(req): Json<AddRouteRequest>) -> Json<Value> {
    match crate::services::frr::add_route(
        &req.prefix,
        req.nexthop.as_deref(),
        req.interface.as_deref(),
        req.distance,
    ) {
        Ok(msg) => Json(json!({ "status": "ok", "message": msg })),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn del_frr_route(Json(req): Json<DelRouteRequest>) -> Json<Value> {
    match crate::services::frr::del_route(
        &req.prefix,
        req.nexthop.as_deref(),
        req.interface.as_deref(),
        req.distance,
    ) {
        Ok(msg) => Json(json!({ "status": "ok", "message": msg })),
        Err(e) => Json(json!({ "error": e })),
    }
}

// ── DHCP handlers (native Rust) ────────────────────────────────────

pub async fn get_dhcp_status() -> Json<Value> {
    match crate::services::dhcp::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn enable_dhcp() -> Json<Value> {
    let config = crate::services::dhcp::DhcpEnableConfig::default();
    match crate::services::dhcp::enable(config) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Log management handlers (native Rust) ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub sources: Option<String>,
    pub level: Option<String>,
    pub lines: Option<u32>,
    pub filter: Option<String>,
    pub limit: Option<u32>,
}

pub async fn get_logs(Json(query): Json<LogQuery>) -> Json<Value> {
    let q = crate::services::logs::LogQuery {
        sources: query.sources,
        level: query.level,
        lines: query.lines,
        filter: query.filter,
        limit: query.limit,
    };

    match crate::services::logs::show(q) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn clear_logs() -> Json<Value> {
    match crate::services::logs::clear(None) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall management handlers (native Rust) ─────────────────────

#[derive(Debug, Deserialize)]
pub struct FirewallRuleRequest {
    pub action: String,
    pub src_ip: Option<String>,
    pub dst_ip: Option<String>,
    pub src_port: Option<u32>,
    pub dst_port: Option<u32>,
    pub protocol: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FirewallRuleDelete {
    pub id: u32,
}

pub async fn get_firewall_status() -> Json<Value> {
    match crate::services::firewall::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn add_firewall_rule(Json(req): Json<FirewallRuleRequest>) -> Json<Value> {
    let rule_req = crate::services::firewall::AddRuleRequest {
        action: req.action,
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        protocol: req.protocol,
        description: req.description,
    };

    match crate::services::firewall::add_rule(rule_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_firewall_rule(Json(req): Json<FirewallRuleDelete>) -> Json<Value> {
    match crate::services::firewall::del_rule(req.id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn enable_firewall() -> Json<Value> {
    match crate::services::firewall::enable() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn disable_firewall() -> Json<Value> {
    match crate::services::firewall::disable() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── IPv6 handlers (native Rust) ────────────────────────────────────

pub async fn get_ipv6_status() -> Json<Value> {
    match crate::services::ipv6::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_ipv6_neighbors() -> Json<Value> {
    match crate::services::ipv6::show_ndp() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_dhcpv6_status() -> Json<Value> {
    // DHCPv6 is not yet implemented natively; return a stub response
    Json(json!({
        "status": "inactive",
        "message": "DHCPv6 management not yet implemented"
    }))
}

// ── QoS management handlers (native Rust) ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatePolicerReq {
    pub name: String,
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_policer_type")]
    pub policer_type: String,
}

fn default_policer_type() -> String {
    "single_rate_two_color".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SetInterfaceLimitReq {
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_limit_direction")]
    pub direction: String,
}

fn default_limit_direction() -> String {
    "both".to_string()
}

pub async fn get_qos_status() -> Json<Value> {
    match crate::services::qos::show_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn create_policer(Json(req): Json<CreatePolicerReq>) -> Json<Value> {
    let qos_req = crate::services::qos::CreatePolicerRequest {
        name: req.name,
        rate: req.rate,
        burst: req.burst,
        policer_type: req.policer_type,
    };
    match crate::services::qos::create_policer(qos_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_policer(Path(name): Path<String>) -> Json<Value> {
    match crate::services::qos::delete_policer(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn set_interface_rate_limit(
    Path(name): Path<String>,
    Json(req): Json<SetInterfaceLimitReq>,
) -> Json<Value> {
    let qos_req = crate::services::qos::SetInterfaceLimitRequest {
        rate: req.rate,
        burst: req.burst,
        direction: req.direction,
    };
    match crate::services::qos::set_interface_limit(&name, qos_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Flow monitoring handlers ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FlowExportSetRequest {
    pub collector_ip: String,
    pub collector_port: u32,
}

pub async fn get_flow_status() -> Json<Value> {
    match crate::services::flow::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_flow_top() -> Json<Value> {
    match crate::services::flow::get_top_talkers() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn set_flow_export(Json(req): Json<FlowExportSetRequest>) -> Json<Value> {
    match crate::services::flow::set_export_collector(&req.collector_ip, req.collector_port) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn enable_flow_export() -> Json<Value> {
    match crate::services::flow::enable_export() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn disable_flow_export() -> Json<Value> {
    match crate::services::flow::disable_export() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn setup_flow_classify() -> Json<Value> {
    match crate::services::flow::setup_classify() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn list_flows() -> Json<Value> {
    match crate::services::flow::list_flows() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Backup management handlers ─────────────────────────────────────

