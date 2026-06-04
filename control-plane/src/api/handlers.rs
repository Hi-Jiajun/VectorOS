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
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    #[serde(default)]
    pub src_port: Option<String>,
    #[serde(default)]
    pub dst_port: Option<String>,
    #[serde(default)]
    pub src_alias: Option<String>,
    #[serde(default)]
    pub dst_alias: Option<String>,
    #[serde(default)]
    pub src_port_alias: Option<String>,
    #[serde(default)]
    pub dst_port_alias: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub log: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub log_prefix: Option<String>,
    #[serde(default)]
    pub geoip_countries: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FirewallRuleDelete {
    pub id: u32,
}

#[derive(Debug, Deserialize)]
pub struct FirewallRuleUpdate {
    pub id: u32,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    #[serde(default)]
    pub src_port: Option<String>,
    #[serde(default)]
    pub dst_port: Option<String>,
    #[serde(default)]
    pub src_alias: Option<String>,
    #[serde(default)]
    pub dst_alias: Option<String>,
    #[serde(default)]
    pub src_port_alias: Option<String>,
    #[serde(default)]
    pub dst_port_alias: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub log: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub geoip_countries: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderRulesReq {
    pub rule_ids: Vec<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AddGroupReq {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true_bool")]
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
}

fn default_true_bool() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct GroupRuleReq {
    pub rule_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct AddAliasReq {
    pub name: String,
    #[serde(rename = "type")]
    pub alias_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true_bool")]
    pub enabled: bool,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub refresh_interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAliasReq {
    #[serde(default)]
    pub entries: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddScheduleReq {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true_bool")]
    pub enabled: bool,
    #[serde(default)]
    pub time_ranges: Vec<crate::services::firewall::TimeRange>,
}

#[derive(Debug, Deserialize)]
pub struct GeoIpReq {
    pub enabled: bool,
    #[serde(default)]
    pub default_action: String,
    #[serde(default)]
    pub blocked_countries: Vec<String>,
    #[serde(default)]
    pub allowed_countries: Vec<String>,
    #[serde(default)]
    pub db_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShaperIfaceReq {
    pub interface: String,
    pub bandwidth: u64,
    #[serde(default)]
    pub download: Option<u64>,
    #[serde(default)]
    pub upload: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ShaperQueueReq {
    pub name: String,
    pub weight: u32,
    pub priority: u32,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IdsConfigReq {
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub rule_categories: Option<std::collections::HashMap<String, bool>>,
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
        direction: req.direction,
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        src_alias: req.src_alias,
        dst_alias: req.dst_alias,
        src_port_alias: req.src_port_alias,
        dst_port_alias: req.dst_port_alias,
        protocol: req.protocol,
        group: req.group,
        schedule: req.schedule,
        log: req.log,
        description: req.description,
        dscp: req.dscp,
        log_prefix: req.log_prefix,
        geoip_countries: req.geoip_countries,
    };

    match crate::services::firewall::add_rule(rule_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn update_firewall_rule(Json(req): Json<FirewallRuleUpdate>) -> Json<Value> {
    let rule_req = crate::services::firewall::UpdateRuleRequest {
        id: req.id,
        action: req.action,
        enabled: req.enabled,
        direction: req.direction,
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        src_alias: req.src_alias,
        dst_alias: req.dst_alias,
        src_port_alias: req.src_port_alias,
        dst_port_alias: req.dst_port_alias,
        protocol: req.protocol,
        group: req.group,
        schedule: req.schedule,
        log: req.log,
        description: req.description,
        dscp: req.dscp,
        geoip_countries: req.geoip_countries,
    };

    match crate::services::firewall::update_rule(rule_req) {
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

pub async fn reorder_firewall_rules(Json(req): Json<ReorderRulesReq>) -> Json<Value> {
    let reorder_req = crate::services::firewall::ReorderRequest {
        rule_ids: req.rule_ids,
    };
    match crate::services::firewall::reorder_rules(reorder_req) {
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

// ── Firewall Groups ─────────────────────────────────────────────────

pub async fn add_firewall_group(Json(req): Json<AddGroupReq>) -> Json<Value> {
    let group_req = crate::services::firewall::AddGroupRequest {
        name: req.name,
        description: req.description,
        enabled: req.enabled,
        interfaces: req.interfaces,
    };
    match crate::services::firewall::add_group(group_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_firewall_group(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_group(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn add_rule_to_group(
    Path(group_name): Path<String>,
    Json(req): Json<GroupRuleReq>,
) -> Json<Value> {
    match crate::services::firewall::add_rule_to_group(&group_name, req.rule_id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn remove_rule_from_group(
    Path(group_name): Path<String>,
    Json(req): Json<GroupRuleReq>,
) -> Json<Value> {
    match crate::services::firewall::remove_rule_from_group(&group_name, req.rule_id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn list_firewall_groups() -> Json<Value> {
    match crate::services::firewall::list_groups() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall Aliases ────────────────────────────────────────────────

pub async fn add_firewall_alias(Json(req): Json<AddAliasReq>) -> Json<Value> {
    let alias_req = crate::services::firewall::AddAliasRequest {
        name: req.name,
        alias_type: req.alias_type,
        description: req.description,
        enabled: req.enabled,
        entries: req.entries,
        refresh_interval: req.refresh_interval,
    };
    match crate::services::firewall::add_alias(alias_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn update_firewall_alias(
    Path(name): Path<String>,
    Json(req): Json<UpdateAliasReq>,
) -> Json<Value> {
    match crate::services::firewall::update_alias(&name, req.entries, req.enabled, req.description) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_firewall_alias(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_alias(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn list_firewall_aliases() -> Json<Value> {
    match crate::services::firewall::list_aliases() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn refresh_firewall_alias(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::refresh_url_alias(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall Schedules ──────────────────────────────────────────────

pub async fn add_firewall_schedule(Json(req): Json<AddScheduleReq>) -> Json<Value> {
    let schedule_req = crate::services::firewall::AddScheduleRequest {
        name: req.name,
        description: req.description,
        enabled: req.enabled,
        time_ranges: req.time_ranges,
    };
    match crate::services::firewall::add_schedule(schedule_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_firewall_schedule(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_schedule(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn list_firewall_schedules() -> Json<Value> {
    match crate::services::firewall::list_schedules() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── GeoIP ───────────────────────────────────────────────────────────

pub async fn update_firewall_geoip(Json(req): Json<GeoIpReq>) -> Json<Value> {
    let geoip = crate::services::firewall::GeoIpConfig {
        enabled: req.enabled,
        default_action: req.default_action,
        blocked_countries: req.blocked_countries,
        allowed_countries: req.allowed_countries,
        db_path: req.db_path,
    };
    match crate::services::firewall::update_geoip(geoip) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Traffic Shaper ──────────────────────────────────────────────────

pub async fn set_shaper_interface(Json(req): Json<ShaperIfaceReq>) -> Json<Value> {
    let shaper_req = crate::services::firewall::ShaperIfaceRequest {
        interface: req.interface,
        bandwidth: req.bandwidth,
        download: req.download,
        upload: req.upload,
    };
    match crate::services::firewall::set_shaper_interface(shaper_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn remove_shaper_interface(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::remove_shaper_interface(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn add_shaper_queue(Json(req): Json<ShaperQueueReq>) -> Json<Value> {
    let queue_req = crate::services::firewall::ShaperQueueRequest {
        name: req.name,
        weight: req.weight,
        priority: req.priority,
        dscp: req.dscp,
        interface: req.interface,
        description: req.description,
    };
    match crate::services::firewall::add_shaper_queue(queue_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_shaper_queue(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_shaper_queue(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_shaper_status() -> Json<Value> {
    match crate::services::firewall::get_shaper_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── IDS / Suricata ──────────────────────────────────────────────────

pub async fn update_ids_config(Json(req): Json<IdsConfigReq>) -> Json<Value> {
    let ids_req = crate::services::firewall::IdsConfigRequest {
        enabled: req.enabled,
        interfaces: req.interfaces,
        rule_categories: req.rule_categories,
    };
    match crate::services::firewall::update_ids(ids_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_ids_alerts() -> Json<Value> {
    match crate::services::firewall::get_ids_alerts(Some(100)) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn clear_ids_alerts() -> Json<Value> {
    match crate::services::firewall::clear_ids_alerts() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_ids_stats() -> Json<Value> {
    match crate::services::firewall::get_ids_stats() {
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

// ── Connection tracking handlers ────────────────────────────────────

pub async fn get_conntrack_status() -> Json<Value> {
    match crate::services::conntrack::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_conntrack_connections() -> Json<Value> {
    match crate::services::conntrack::list_connections() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_conntrack_stats() -> Json<Value> {
    match crate::services::conntrack::get_stats() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_conntrack_top() -> Json<Value> {
    match crate::services::conntrack::get_top_talkers() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct ConntrackFilterRequest {
    pub ip: Option<String>,
    pub port: Option<u32>,
    pub protocol: Option<String>,
}

pub async fn filter_conntrack_connections(Json(req): Json<ConntrackFilterRequest>) -> Json<Value> {
    let filter = crate::services::conntrack::ConntrackFilter {
        ip: req.ip,
        port: req.port,
        protocol: req.protocol,
    };
    match crate::services::conntrack::filter_connections(&filter) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_conntrack_detail() -> Json<Value> {
    match crate::services::conntrack::get_nat_detail() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Backup management handlers ─────────────────────────────────────

// ── Traffic control handlers (native Rust) ────────────────────────

#[derive(Debug, Deserialize)]
pub struct TrafficLimitRequest {
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_traffic_direction")]
    pub direction: String,
}

fn default_traffic_direction() -> String {
    "both".to_string()
}

#[derive(Debug, Deserialize)]
pub struct TrafficIpLimitRequest {
    pub ip: String,
    pub rate: u64,
    pub burst: u64,
}

#[derive(Debug, Deserialize)]
pub struct TrafficPriorityRequest {
    pub name: String,
    pub queue: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TrafficAppClassRequest {
    pub name: String,
    #[serde(default)]
    pub ports: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default = "default_traffic_priority")]
    pub priority: String,
    pub dscp: Option<u32>,
    pub description: Option<String>,
}

fn default_traffic_priority() -> String {
    "medium".to_string()
}

#[derive(Debug, Deserialize)]
pub struct TrafficIpDeleteRequest {
    pub ip: String,
}

/// GET /api/traffic/status
pub async fn get_traffic_status() -> Json<Value> {
    match crate::services::traffic::show_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/traffic/limit
/// Set bandwidth limit on an interface or IP.
/// Body: { "type": "interface"|"ip", "target": "..."|"...", ... }
#[derive(Debug, Deserialize)]
pub struct TrafficLimitBody {
    #[serde(rename = "type")]
    pub limit_type: String,
    pub target: Option<String>,
    pub ip: Option<String>,
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_traffic_direction")]
    pub direction: String,
}

pub async fn set_traffic_limit(Json(body): Json<TrafficLimitBody>) -> Json<Value> {
    match body.limit_type.as_str() {
        "interface" => {
            let target = match body.target {
                Some(t) => t,
                None => return Json(json!({ "error": "target (interface name) is required" })),
            };
            let req = crate::services::traffic::SetInterfaceLimitRequest {
                rate: body.rate,
                burst: body.burst,
                direction: body.direction,
            };
            match crate::services::traffic::set_interface_limit(&target, req) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": e.to_string() })),
            }
        }
        "ip" => {
            let ip = match body.ip {
                Some(i) => i,
                None => match body.target {
                    Some(t) => t,
                    None => return Json(json!({ "error": "ip is required" })),
                },
            };
            let req = crate::services::traffic::SetIpLimitRequest {
                ip,
                rate: body.rate,
                burst: body.burst,
            };
            match crate::services::traffic::set_ip_limit(req) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": e.to_string() })),
            }
        }
        _ => Json(json!({ "error": "Invalid limit type. Use 'interface' or 'ip'" })),
    }
}

/// DELETE /api/traffic/limit/interface/:iface
pub async fn remove_traffic_interface_limit(Path(iface): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_interface_limit(&iface) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// DELETE /api/traffic/limit/ip/:ip
pub async fn remove_traffic_ip_limit(Path(ip): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_ip_limit(&ip) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/traffic/priority
pub async fn set_traffic_priority(Json(req): Json<TrafficPriorityRequest>) -> Json<Value> {
    let traffic_req = crate::services::traffic::SetPriorityRequest {
        name: req.name,
        queue: req.queue,
        description: req.description,
    };
    match crate::services::traffic::set_priority(traffic_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/traffic/app-class
pub async fn set_traffic_app_class(Json(req): Json<TrafficAppClassRequest>) -> Json<Value> {
    let traffic_req = crate::services::traffic::SetAppClassRequest {
        name: req.name,
        ports: req.ports,
        protocol: req.protocol,
        priority: req.priority,
        dscp: req.dscp,
        description: req.description,
    };
    match crate::services::traffic::set_app_class(traffic_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// DELETE /api/traffic/app-class/:name
pub async fn remove_traffic_app_class(Path(name): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_app_class(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/traffic/defaults
pub async fn load_traffic_defaults() -> Json<Value> {
    match crate::services::traffic::load_defaults() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// GET /api/traffic/stats
pub async fn get_traffic_stats() -> Json<Value> {
    match crate::services::traffic::get_stats() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/traffic/reset
pub async fn reset_traffic() -> Json<Value> {
    match crate::services::traffic::reset() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── VPN management handlers ────────────────────────────────────────

pub async fn get_vpn_status() -> Json<Value> {
    match crate::services::vpn::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn get_vpn_connections() -> Json<Value> {
    match crate::services::vpn::list_connections() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct VpnDownRequest {
    pub vpn_type: String,
    pub name: String,
}

pub async fn configure_wireguard(Json(req): Json<crate::services::vpn::WireGuardConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_wireguard(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn configure_ipsec(Json(req): Json<crate::services::vpn::IpsecConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_ipsec(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn configure_openvpn(Json(req): Json<crate::services::vpn::OpenVpnConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_openvpn(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn vpn_down(Json(req): Json<VpnDownRequest>) -> Json<Value> {
    match crate::services::vpn::bring_down(&req.vpn_type, &req.name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Network diagnostics handlers (native Rust) ──────────────────────

pub async fn get_diag_status() -> Json<Value> {
    match crate::services::diag::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn diag_ping(Json(req): Json<crate::services::diag::PingRequest>) -> Json<Value> {
    match crate::services::diag::ping(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn diag_traceroute(Json(req): Json<crate::services::diag::TracerouteRequest>) -> Json<Value> {
    match crate::services::diag::traceroute(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn diag_dns(Json(req): Json<crate::services::diag::DnsRequest>) -> Json<Value> {
    match crate::services::diag::dns_lookup(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

pub async fn diag_portscan(Json(req): Json<crate::services::diag::PortScanRequest>) -> Json<Value> {
    match crate::services::diag::port_scan(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── VyOS-style configuration management handlers ────────────────

/// GET /api/config/tree
/// Get the full configuration as a hierarchical tree.
pub async fn get_config_tree() -> Json<Value> {
    let tree = crate::services::config_cli::get_tree();
    Json(json!({
        "status": "ok",
        "tree": tree
    }))
}

/// GET /api/config/staging
/// Get the staged (uncommitted) configuration tree.
pub async fn get_config_staging() -> Json<Value> {
    match crate::services::config_cli::get_staging_tree() {
        Some(staging) => Json(json!({
            "status": "ok",
            "staging": staging
        })),
        None => Json(json!({
            "status": "ok",
            "staging": null,
            "message": "No staged changes"
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigSetRequest {
    pub path: String,
    pub value: serde_json::Value,
}

/// POST /api/config/set
/// Set a configuration value at a dot-separated path (staged).
pub async fn config_set_value(Json(req): Json<ConfigSetRequest>) -> Json<Value> {
    match crate::services::config_cli::set_value(&req.path, req.value) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigDeleteRequest {
    pub path: String,
}

/// POST /api/config/delete
/// Delete a configuration value at a dot-separated path (staged).
pub async fn config_delete_value(Json(req): Json<ConfigDeleteRequest>) -> Json<Value> {
    match crate::services::config_cli::delete_value(&req.path) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/config/commit
/// Commit all staged configuration changes.
pub async fn config_commit() -> Json<Value> {
    match crate::services::config_cli::commit() {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/config/rollback/:version
/// Rollback configuration to a specific version.
pub async fn config_rollback(Path(version): Path<String>) -> Json<Value> {
    match crate::services::config_cli::rollback(&version) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/config/discard
/// Discard staged (uncommitted) changes.
pub async fn config_discard() -> Json<Value> {
    let result = crate::services::config_cli::execute_cli("", "discard");
    Json(json!(result))
}

/// GET /api/config/diff
/// Get diff between committed and staged configuration.
pub async fn config_diff() -> Json<Value> {
    let diff = crate::services::config_cli::get_diff();
    Json(json!({
        "status": "ok",
        "changes": diff.len(),
        "diff": diff
    }))
}

/// GET /api/config/diff/:v1/:v2
/// Get diff between two specific config versions.
pub async fn config_diff_versions(
    Path((v1, v2)): Path<(String, String)>,
) -> Json<Value> {
    match crate::services::config_cli::get_diff_versions(&v1, &v2) {
        Ok(diff) => Json(json!({
            "status": "ok",
            "changes": diff.len(),
            "diff": diff
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// GET /api/config/history
/// Get configuration version history.
pub async fn config_history() -> Json<Value> {
    let history = crate::services::config_cli::list_history();
    Json(json!({
        "status": "ok",
        "history": history,
        "count": history.len()
    }))
}

/// GET /api/config/templates
/// List saved configuration templates.
pub async fn config_list_templates() -> Json<Value> {
    let templates = crate::services::config_cli::list_templates();
    Json(json!({
        "status": "ok",
        "templates": templates,
        "count": templates.len()
    }))
}

#[derive(Debug, Deserialize)]
pub struct SaveTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// POST /api/config/template/save
/// Save current configuration as a named template.
pub async fn config_save_template(Json(req): Json<SaveTemplateRequest>) -> Json<Value> {
    let tree = crate::services::config_cli::get_tree();
    match crate::services::config_cli::save_template(&req.name, &req.description, &tree) {
        Ok(()) => Json(json!({
            "status": "ok",
            "message": format!("Template '{}' saved", req.name)
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct ApplyTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

/// POST /api/config/template/apply
/// Apply a named template (with optional variable substitution) to staging.
pub async fn config_apply_template(Json(req): Json<ApplyTemplateRequest>) -> Json<Value> {
    let vars = if req.variables.is_empty() {
        None
    } else {
        Some(&req.variables)
    };
    match crate::services::config_cli::apply_template(&req.name, vars) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize)]
pub struct CliSessionRequest {
    #[serde(default)]
    pub command: Option<String>,
}

/// POST /api/config/cli/session
/// Create a new CLI session or execute a command in an existing session.
pub async fn config_cli_session(Json(req): Json<CliSessionRequest>) -> Json<Value> {
    let session = crate::services::config_cli::create_session();
    if let Some(cmd) = &req.command {
        let result = crate::services::config_cli::execute_cli(&session.id, cmd);
        Json(json!({
            "status": "ok",
            "session": session,
            "result": result
        }))
    } else {
        Json(json!({
            "status": "ok",
            "session": session
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct CliExecuteRequest {
    pub session_id: String,
    pub command: String,
}

/// POST /api/config/cli/execute
/// Execute a CLI command in an existing session.
pub async fn config_cli_execute(Json(req): Json<CliExecuteRequest>) -> Json<Value> {
    let result = crate::services::config_cli::execute_cli(&req.session_id, &req.command);
    Json(json!(result))
}

