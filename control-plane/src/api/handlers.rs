use axum::extract::{State, Path, Query};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;
use utoipa::ToSchema;
use crate::api::AppState;
use crate::auth::{LoginRequest, LoginResponse};

#[derive(Debug, Deserialize, ToSchema)]
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

/// Authenticate and obtain a JWT token.
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "Authentication",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = Value),
        (status = 401, description = "Invalid credentials", body = Value)
    )
)]
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

/// Check system health status.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "System",
    responses(
        (status = 200, description = "System is healthy", body = Value)
    )
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Get the current router configuration.
#[utoipa::path(
    get,
    path = "/api/config",
    tag = "Configuration",
    responses(
        (status = 200, description = "Current configuration", body = Value)
    )
)]
pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "config": state.config }))
}

/// List all network interfaces.
#[utoipa::path(
    get,
    path = "/api/interfaces",
    tag = "Interfaces",
    responses(
        (status = 200, description = "List of interfaces", body = Value)
    )
)]
pub async fn get_interfaces() -> Json<Value> {
    match crate::vpp::native::get_interfaces() {
        Ok(interfaces) => Json(json!({ "interfaces": interfaces })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Bring up a network interface.
#[utoipa::path(
    post,
    path = "/api/interfaces/{name}/up",
    tag = "Interfaces",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    responses(
        (status = 200, description = "Interface brought up", body = Value)
    )
)]
pub async fn iface_up(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::set_interface_state(&name, "up") {
        Ok(()) => Json(json!({ "status": "ok", "message": format!("Interface {} set to up", name) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Bring down a network interface.
#[utoipa::path(
    post,
    path = "/api/interfaces/{name}/down",
    tag = "Interfaces",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    responses(
        (status = 200, description = "Interface brought down", body = Value)
    )
)]
pub async fn iface_down(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::set_interface_state(&name, "down") {
        Ok(()) => Json(json!({ "status": "ok", "message": format!("Interface {} set to down", name) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InterfaceConfigRequest {
    pub mtu: Option<u32>,
    pub ip_add: Option<String>,
    pub ip_remove: Option<String>,
    pub promiscuous: Option<bool>,
}

/// Configure interface: MTU, IP add/remove, promiscuous mode.
#[utoipa::path(
    post,
    path = "/api/interfaces/{name}/config",
    tag = "Interfaces",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    request_body = InterfaceConfigRequest,
    responses(
        (status = 200, description = "Configuration applied", body = Value)
    )
)]
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

/// Get detailed interface statistics (packets, bytes, errors, drops).
#[utoipa::path(
    get,
    path = "/api/interfaces/{name}/stats",
    tag = "Interfaces",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    responses(
        (status = 200, description = "Interface statistics", body = Value)
    )
)]
pub async fn get_interface_stats(Path(name): Path<String>) -> Json<Value> {
    match crate::vpp::native::get_interface_stats(&name) {
        Ok(stats) => Json(json!({ "stats": stats })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all PPPoE clients.
#[utoipa::path(
    get,
    path = "/api/pppoe/clients",
    tag = "PPPoE",
    responses(
        (status = 200, description = "List of PPPoE clients", body = Value)
    )
)]
pub async fn get_pppoe_clients() -> Json<Value> {
    match run_vpp_cmd("dump", &[]) {
        Ok(data) => Json(json!({ "clients": data })),
        Err(e) => Json(json!({ "error": e })),
    }
}

/// Get PPPoE connection status.
#[utoipa::path(
    get,
    path = "/api/pppoe/status",
    tag = "PPPoE",
    responses(
        (status = 200, description = "PPPoE status", body = Value)
    )
)]
pub async fn get_pppoe_status() -> Json<Value> {
    match crate::vpp::native::get_pppoe_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Create a new PPPoE client connection.
#[utoipa::path(
    post,
    path = "/api/pppoe/create",
    tag = "PPPoE",
    request_body = PppoeConfig,
    responses(
        (status = 200, description = "PPPoE client created", body = Value)
    )
)]
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

/// Get NAT status.
#[utoipa::path(
    get,
    path = "/api/nat/status",
    tag = "NAT",
    responses(
        (status = 200, description = "NAT status", body = Value)
    )
)]
pub async fn get_nat_status() -> Json<Value> {
    match crate::vpp::native::get_nat_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Enable NAT on configured inside/outside interfaces.
#[utoipa::path(
    post,
    path = "/api/nat/enable",
    tag = "NAT",
    responses(
        (status = 200, description = "NAT enabled", body = Value)
    )
)]
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

/// Get the VPP routing table.
#[utoipa::path(
    get,
    path = "/api/routes",
    tag = "Routes",
    responses(
        (status = 200, description = "Routing table", body = Value)
    )
)]
pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

/// Get comprehensive system status including CPU, memory, disk, and VPP info.
#[utoipa::path(
    get,
    path = "/api/system",
    tag = "System",
    responses(
        (status = 200, description = "System status", body = Value)
    )
)]
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

/// Get VPP data plane performance metrics.
#[utoipa::path(
    get,
    path = "/api/system/vpp-performance",
    tag = "System",
    responses(
        (status = 200, description = "VPP performance metrics", body = Value)
    )
)]
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

/// Get the VPP configuration status from the config manager.
#[utoipa::path(
    get,
    path = "/api/config/status",
    tag = "System",
    responses(
        (status = 200, description = "Config status", body = Value)
    )
)]
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

/// Save the full router configuration.
#[utoipa::path(
    post,
    path = "/api/config/save",
    tag = "System",
    request_body = Value,
    responses(
        (status = 200, description = "Configuration saved", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct DnsEnableRequest {
    pub upstream: Option<String>,
    pub upstream_v6: Option<String>,
    pub interface: Option<String>,
    pub cache_size: Option<u32>,
}

/// Get DNS resolver status.
#[utoipa::path(
    get,
    path = "/api/dns/status",
    tag = "DNS",
    responses(
        (status = 200, description = "DNS status", body = Value)
    )
)]
pub async fn get_dns_status() -> Json<Value> {
    match crate::services::dns::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Enable the DNS resolver with the provided configuration.
#[utoipa::path(
    post,
    path = "/api/dns/enable",
    tag = "DNS",
    request_body = DnsEnableRequest,
    responses(
        (status = 200, description = "DNS enabled", body = Value)
    )
)]
pub async fn enable_dns(Json(req): Json<DnsEnableRequest>) -> Json<Value> {
    let config = crate::services::dns::DnsEnableConfig {
        upstream: req.upstream.unwrap_or_default(),
        upstream_v6: req.upstream_v6.unwrap_or_default(),
        interface: req.interface.unwrap_or_default(),
        cache_size: req.cache_size.unwrap_or_default(),
    };
    match crate::services::dns::enable(config) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── FRRouting handlers (native Rust) ───────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddRouteRequest {
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub distance: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DelRouteRequest {
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub distance: Option<u32>,
}

/// Get FRRouting daemon status.
#[utoipa::path(
    get,
    path = "/api/frr/status",
    tag = "FRRouting",
    responses(
        (status = 200, description = "FRR status", body = Value)
    )
)]
pub async fn get_frr_status() -> Json<Value> {
    match crate::services::frr::get_status() {
        Ok(status) => Json(json!(status)),
        Err(e) => Json(json!({ "error": e })),
    }
}

/// List all routes learned via FRRouting.
#[utoipa::path(
    get,
    path = "/api/frr/routes",
    tag = "FRRouting",
    responses(
        (status = 200, description = "FRR routes", body = Value)
    )
)]
pub async fn get_frr_routes() -> Json<Value> {
    match crate::services::frr::show_routes() {
        Ok(routes) => Json(json!({ "routes": routes })),
        Err(e) => Json(json!({ "error": e })),
    }
}

/// Add a static route via FRRouting.
#[utoipa::path(
    post,
    path = "/api/frr/add-route",
    tag = "FRRouting",
    request_body = AddRouteRequest,
    responses(
        (status = 200, description = "Route added", body = Value)
    )
)]
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

/// Delete a static route via FRRouting.
#[utoipa::path(
    post,
    path = "/api/frr/del-route",
    tag = "FRRouting",
    request_body = DelRouteRequest,
    responses(
        (status = 200, description = "Route deleted", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct DhcpEnableRequest {
    pub interface: Option<String>,
    pub start_ip: Option<String>,
    pub end_ip: Option<String>,
    pub gateway: Option<String>,
    pub lease_time: Option<u32>,
    pub dns_servers: Option<String>,
}

/// Get DHCP server status and active leases.
#[utoipa::path(
    get,
    path = "/api/dhcp/status",
    tag = "DHCP",
    responses(
        (status = 200, description = "DHCP status", body = Value)
    )
)]
pub async fn get_dhcp_status() -> Json<Value> {
    match crate::services::dhcp::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Enable the DHCP server with the provided configuration.
#[utoipa::path(
    post,
    path = "/api/dhcp/enable",
    tag = "DHCP",
    request_body = DhcpEnableRequest,
    responses(
        (status = 200, description = "DHCP enabled", body = Value)
    )
)]
pub async fn enable_dhcp(Json(req): Json<DhcpEnableRequest>) -> Json<Value> {
    let config = crate::services::dhcp::DhcpEnableConfig {
        interface: req.interface.unwrap_or_default(),
        start_ip: req.start_ip.unwrap_or_default(),
        end_ip: req.end_ip.unwrap_or_default(),
        gateway: req.gateway.unwrap_or_default(),
        lease_time: req.lease_time.unwrap_or_default(),
        dns_servers: req.dns_servers,
    };
    match crate::services::dhcp::enable(config) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Log management handlers (native Rust) ──────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogQuery {
    pub sources: Option<String>,
    pub level: Option<String>,
    pub lines: Option<u32>,
    pub filter: Option<String>,
    pub limit: Option<u32>,
}

/// Query system logs with optional filtering.
#[utoipa::path(
    post,
    path = "/api/logs",
    tag = "Logs",
    request_body = LogQuery,
    responses(
        (status = 200, description = "Log entries", body = Value)
    )
)]
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

/// Clear all system logs.
#[utoipa::path(
    post,
    path = "/api/logs/clear",
    tag = "Logs",
    responses(
        (status = 200, description = "Logs cleared", body = Value)
    )
)]
pub async fn clear_logs() -> Json<Value> {
    match crate::services::logs::clear(None) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall management handlers (native Rust) ─────────────────────

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct FirewallRuleDelete {
    pub id: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReorderRulesReq {
    pub rule_ids: Vec<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct GroupRuleReq {
    pub rule_id: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAliasReq {
    #[serde(default)]
    pub entries: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddScheduleReq {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true_bool")]
    pub enabled: bool,
    #[serde(default)]
    pub time_ranges: Vec<crate::services::firewall::TimeRange>,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ShaperIfaceReq {
    pub interface: String,
    pub bandwidth: u64,
    #[serde(default)]
    pub download: Option<u64>,
    #[serde(default)]
    pub upload: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct IdsConfigReq {
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub rule_categories: Option<std::collections::HashMap<String, bool>>,
}

/// Get firewall status including rules, groups, and aliases.
#[utoipa::path(
    get,
    path = "/api/firewall/status",
    tag = "Firewall",
    responses(
        (status = 200, description = "Firewall status", body = Value)
    )
)]
pub async fn get_firewall_status() -> Json<Value> {
    match crate::services::firewall::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Add a new firewall rule.
#[utoipa::path(
    post,
    path = "/api/firewall/add-rule",
    tag = "Firewall",
    request_body = FirewallRuleRequest,
    responses(
        (status = 200, description = "Rule added", body = Value)
    )
)]
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

/// Update an existing firewall rule.
#[utoipa::path(
    post,
    path = "/api/firewall/update-rule",
    tag = "Firewall",
    request_body = FirewallRuleUpdate,
    responses(
        (status = 200, description = "Rule updated", body = Value)
    )
)]
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

/// Delete a firewall rule by ID.
#[utoipa::path(
    post,
    path = "/api/firewall/del-rule",
    tag = "Firewall",
    request_body = FirewallRuleDelete,
    responses(
        (status = 200, description = "Rule deleted", body = Value)
    )
)]
pub async fn delete_firewall_rule(Json(req): Json<FirewallRuleDelete>) -> Json<Value> {
    match crate::services::firewall::del_rule(req.id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Reorder firewall rules by specifying the new order of rule IDs.
#[utoipa::path(
    post,
    path = "/api/firewall/reorder",
    tag = "Firewall",
    request_body = ReorderRulesReq,
    responses(
        (status = 200, description = "Rules reordered", body = Value)
    )
)]
pub async fn reorder_firewall_rules(Json(req): Json<ReorderRulesReq>) -> Json<Value> {
    let reorder_req = crate::services::firewall::ReorderRequest {
        rule_ids: req.rule_ids,
    };
    match crate::services::firewall::reorder_rules(reorder_req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Enable the firewall.
#[utoipa::path(
    post,
    path = "/api/firewall/enable",
    tag = "Firewall",
    responses(
        (status = 200, description = "Firewall enabled", body = Value)
    )
)]
pub async fn enable_firewall() -> Json<Value> {
    match crate::services::firewall::enable() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Disable the firewall.
#[utoipa::path(
    post,
    path = "/api/firewall/disable",
    tag = "Firewall",
    responses(
        (status = 200, description = "Firewall disabled", body = Value)
    )
)]
pub async fn disable_firewall() -> Json<Value> {
    match crate::services::firewall::disable() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall Groups ─────────────────────────────────────────────────

/// Add a new firewall rule group.
#[utoipa::path(
    post,
    path = "/api/firewall/groups/add",
    tag = "Firewall",
    request_body = AddGroupReq,
    responses(
        (status = 200, description = "Group added", body = Value)
    )
)]
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

/// Delete a firewall rule group by name.
#[utoipa::path(
    post,
    path = "/api/firewall/groups/{name}/delete",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    responses(
        (status = 200, description = "Group deleted", body = Value)
    )
)]
pub async fn delete_firewall_group(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_group(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Add a rule to a firewall group.
#[utoipa::path(
    post,
    path = "/api/firewall/groups/{name}/add-rule",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    request_body = GroupRuleReq,
    responses(
        (status = 200, description = "Rule added to group", body = Value)
    )
)]
pub async fn add_rule_to_group(
    Path(group_name): Path<String>,
    Json(req): Json<GroupRuleReq>,
) -> Json<Value> {
    match crate::services::firewall::add_rule_to_group(&group_name, req.rule_id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Remove a rule from a firewall group.
#[utoipa::path(
    post,
    path = "/api/firewall/groups/{name}/remove-rule",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    request_body = GroupRuleReq,
    responses(
        (status = 200, description = "Rule removed from group", body = Value)
    )
)]
pub async fn remove_rule_from_group(
    Path(group_name): Path<String>,
    Json(req): Json<GroupRuleReq>,
) -> Json<Value> {
    match crate::services::firewall::remove_rule_from_group(&group_name, req.rule_id) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all firewall rule groups.
#[utoipa::path(
    get,
    path = "/api/firewall/groups",
    tag = "Firewall",
    responses(
        (status = 200, description = "List of groups", body = Value)
    )
)]
pub async fn list_firewall_groups() -> Json<Value> {
    match crate::services::firewall::list_groups() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall Aliases ────────────────────────────────────────────────

/// Add a new firewall alias (IP/port/network/URL list).
#[utoipa::path(
    post,
    path = "/api/firewall/aliases/add",
    tag = "Firewall",
    request_body = AddAliasReq,
    responses(
        (status = 200, description = "Alias added", body = Value)
    )
)]
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

/// Update an existing firewall alias.
#[utoipa::path(
    post,
    path = "/api/firewall/aliases/{name}",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Alias name")
    ),
    request_body = UpdateAliasReq,
    responses(
        (status = 200, description = "Alias updated", body = Value)
    )
)]
pub async fn update_firewall_alias(
    Path(name): Path<String>,
    Json(req): Json<UpdateAliasReq>,
) -> Json<Value> {
    match crate::services::firewall::update_alias(&name, req.entries, req.enabled, req.description) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Delete a firewall alias by name.
#[utoipa::path(
    post,
    path = "/api/firewall/aliases/{name}/delete",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Alias name")
    ),
    responses(
        (status = 200, description = "Alias deleted", body = Value)
    )
)]
pub async fn delete_firewall_alias(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_alias(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all firewall aliases.
#[utoipa::path(
    get,
    path = "/api/firewall/aliases",
    tag = "Firewall",
    responses(
        (status = 200, description = "List of aliases", body = Value)
    )
)]
pub async fn list_firewall_aliases() -> Json<Value> {
    match crate::services::firewall::list_aliases() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Refresh a URL-based firewall alias to fetch latest entries.
#[utoipa::path(
    post,
    path = "/api/firewall/aliases/{name}/refresh",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Alias name")
    ),
    responses(
        (status = 200, description = "Alias refreshed", body = Value)
    )
)]
pub async fn refresh_firewall_alias(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::refresh_url_alias(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Firewall Schedules ──────────────────────────────────────────────

/// Add a new firewall schedule with time ranges.
#[utoipa::path(
    post,
    path = "/api/firewall/schedules/add",
    tag = "Firewall",
    request_body = AddScheduleReq,
    responses(
        (status = 200, description = "Schedule added", body = Value)
    )
)]
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

/// Delete a firewall schedule by name.
#[utoipa::path(
    post,
    path = "/api/firewall/schedules/{name}/delete",
    tag = "Firewall",
    params(
        ("name" = String, Path, description = "Schedule name")
    ),
    responses(
        (status = 200, description = "Schedule deleted", body = Value)
    )
)]
pub async fn delete_firewall_schedule(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_schedule(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all firewall schedules.
#[utoipa::path(
    get,
    path = "/api/firewall/schedules",
    tag = "Firewall",
    responses(
        (status = 200, description = "List of schedules", body = Value)
    )
)]
pub async fn list_firewall_schedules() -> Json<Value> {
    match crate::services::firewall::list_schedules() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── GeoIP ───────────────────────────────────────────────────────────

/// Update GeoIP filtering configuration.
#[utoipa::path(
    post,
    path = "/api/firewall/geoip",
    tag = "GeoIP",
    request_body = GeoIpReq,
    responses(
        (status = 200, description = "GeoIP config updated", body = Value)
    )
)]
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

/// Set bandwidth limits on a traffic shaper interface.
#[utoipa::path(
    post,
    path = "/api/firewall/shaper/interface",
    tag = "Traffic Shaper",
    request_body = ShaperIfaceReq,
    responses(
        (status = 200, description = "Interface shaper configured", body = Value)
    )
)]
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

/// Remove traffic shaper configuration from an interface.
#[utoipa::path(
    post,
    path = "/api/firewall/shaper/interface/{name}/delete",
    tag = "Traffic Shaper",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    responses(
        (status = 200, description = "Shaper removed", body = Value)
    )
)]
pub async fn remove_shaper_interface(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::remove_shaper_interface(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Add a traffic shaper queue with weight and priority.
#[utoipa::path(
    post,
    path = "/api/firewall/shaper/queue",
    tag = "Traffic Shaper",
    request_body = ShaperQueueReq,
    responses(
        (status = 200, description = "Queue added", body = Value)
    )
)]
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

/// Delete a traffic shaper queue by name.
#[utoipa::path(
    post,
    path = "/api/firewall/shaper/queue/{name}/delete",
    tag = "Traffic Shaper",
    params(
        ("name" = String, Path, description = "Queue name")
    ),
    responses(
        (status = 200, description = "Queue deleted", body = Value)
    )
)]
pub async fn delete_shaper_queue(Path(name): Path<String>) -> Json<Value> {
    match crate::services::firewall::del_shaper_queue(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get traffic shaper status.
#[utoipa::path(
    get,
    path = "/api/firewall/shaper/status",
    tag = "Traffic Shaper",
    responses(
        (status = 200, description = "Shaper status", body = Value)
    )
)]
pub async fn get_shaper_status() -> Json<Value> {
    match crate::services::firewall::get_shaper_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── IDS / Suricata ──────────────────────────────────────────────────

/// Update IDS/IPS (Suricata) configuration.
#[utoipa::path(
    post,
    path = "/api/firewall/ids/config",
    tag = "IDS",
    request_body = IdsConfigReq,
    responses(
        (status = 200, description = "IDS config updated", body = Value)
    )
)]
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

/// Get recent IDS alerts from Suricata.
#[utoipa::path(
    get,
    path = "/api/firewall/ids/alerts",
    tag = "IDS",
    responses(
        (status = 200, description = "IDS alerts", body = Value)
    )
)]
pub async fn get_ids_alerts() -> Json<Value> {
    match crate::services::firewall::get_ids_alerts(Some(100)) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Clear all IDS alerts.
#[utoipa::path(
    post,
    path = "/api/firewall/ids/alerts/clear",
    tag = "IDS",
    responses(
        (status = 200, description = "Alerts cleared", body = Value)
    )
)]
pub async fn clear_ids_alerts() -> Json<Value> {
    match crate::services::firewall::clear_ids_alerts() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get IDS statistics (alert counts, rule hits).
#[utoipa::path(
    get,
    path = "/api/firewall/ids/stats",
    tag = "IDS",
    responses(
        (status = 200, description = "IDS statistics", body = Value)
    )
)]
pub async fn get_ids_stats() -> Json<Value> {
    match crate::services::firewall::get_ids_stats() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── IPv6 handlers (native Rust) ────────────────────────────────────

/// Get IPv6 configuration status.
#[utoipa::path(
    get,
    path = "/api/ipv6/status",
    tag = "IPv6",
    responses(
        (status = 200, description = "IPv6 status", body = Value)
    )
)]
pub async fn get_ipv6_status() -> Json<Value> {
    match crate::services::ipv6::show() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get IPv6 neighbor discovery (NDP) table.
#[utoipa::path(
    get,
    path = "/api/ipv6/neighbors",
    tag = "IPv6",
    responses(
        (status = 200, description = "IPv6 neighbors", body = Value)
    )
)]
pub async fn get_ipv6_neighbors() -> Json<Value> {
    match crate::services::ipv6::show_ndp() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get DHCPv6 client/server status.
#[utoipa::path(
    get,
    path = "/api/dhcpv6/status",
    tag = "IPv6",
    responses(
        (status = 200, description = "DHCPv6 status", body = Value)
    )
)]
pub async fn get_dhcpv6_status() -> Json<Value> {
    // DHCPv6 is not yet implemented natively; return a stub response
    Json(json!({
        "status": "inactive",
        "message": "DHCPv6 management not yet implemented"
    }))
}

// ── QoS management handlers (native Rust) ─────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetInterfaceLimitReq {
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_limit_direction")]
    pub direction: String,
}

fn default_limit_direction() -> String {
    "both".to_string()
}

/// Get QoS status including policers and rate limits.
#[utoipa::path(
    get,
    path = "/api/qos/status",
    tag = "QoS",
    responses(
        (status = 200, description = "QoS status", body = Value)
    )
)]
pub async fn get_qos_status() -> Json<Value> {
    match crate::services::qos::show_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Create a new QoS policer for traffic shaping.
#[utoipa::path(
    post,
    path = "/api/qos/policer",
    tag = "QoS",
    request_body = CreatePolicerReq,
    responses(
        (status = 200, description = "Policer created", body = Value)
    )
)]
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

/// Delete a QoS policer by name.
#[utoipa::path(
    delete,
    path = "/api/qos/policer/{name}",
    tag = "QoS",
    params(
        ("name" = String, Path, description = "Policer name")
    ),
    responses(
        (status = 200, description = "Policer deleted", body = Value)
    )
)]
pub async fn delete_policer(Path(name): Path<String>) -> Json<Value> {
    match crate::services::qos::delete_policer(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Set rate limit on a network interface.
#[utoipa::path(
    post,
    path = "/api/qos/interface/{name}/limit",
    tag = "QoS",
    params(
        ("name" = String, Path, description = "Interface name")
    ),
    request_body = SetInterfaceLimitReq,
    responses(
        (status = 200, description = "Rate limit set", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct FlowExportSetRequest {
    pub collector_ip: String,
    pub collector_port: u32,
}

/// Get flow monitoring status.
#[utoipa::path(
    get,
    path = "/api/flows/status",
    tag = "Flows",
    responses(
        (status = 200, description = "Flow status", body = Value)
    )
)]
pub async fn get_flow_status() -> Json<Value> {
    match crate::services::flow::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get top talkers from flow monitoring.
#[utoipa::path(
    get,
    path = "/api/flows/top",
    tag = "Flows",
    responses(
        (status = 200, description = "Top talkers", body = Value)
    )
)]
pub async fn get_flow_top() -> Json<Value> {
    match crate::services::flow::get_top_talkers() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Configure flow export collector (IPFIX/NetFlow).
#[utoipa::path(
    post,
    path = "/api/flows/export",
    tag = "Flows",
    request_body = FlowExportSetRequest,
    responses(
        (status = 200, description = "Export configured", body = Value)
    )
)]
pub async fn set_flow_export(Json(req): Json<FlowExportSetRequest>) -> Json<Value> {
    match crate::services::flow::set_export_collector(&req.collector_ip, req.collector_port) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Enable flow export to the configured collector.
#[utoipa::path(
    post,
    path = "/api/flows/export/enable",
    tag = "Flows",
    responses(
        (status = 200, description = "Flow export enabled", body = Value)
    )
)]
pub async fn enable_flow_export() -> Json<Value> {
    match crate::services::flow::enable_export() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Disable flow export.
#[utoipa::path(
    post,
    path = "/api/flows/export/disable",
    tag = "Flows",
    responses(
        (status = 200, description = "Flow export disabled", body = Value)
    )
)]
pub async fn disable_flow_export() -> Json<Value> {
    match crate::services::flow::disable_export() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Set up flow classification rules.
#[utoipa::path(
    post,
    path = "/api/flows/classify-setup",
    tag = "Flows",
    responses(
        (status = 200, description = "Classification set up", body = Value)
    )
)]
pub async fn setup_flow_classify() -> Json<Value> {
    match crate::services::flow::setup_classify() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List active flows from the flow monitor.
#[utoipa::path(
    get,
    path = "/api/flows/list",
    tag = "Flows",
    responses(
        (status = 200, description = "Active flows", body = Value)
    )
)]
pub async fn list_flows() -> Json<Value> {
    match crate::services::flow::list_flows() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Connection tracking handlers ────────────────────────────────────

/// Get connection tracking status.
#[utoipa::path(
    get,
    path = "/api/conntrack/status",
    tag = "ConnTrack",
    responses(
        (status = 200, description = "ConnTrack status", body = Value)
    )
)]
pub async fn get_conntrack_status() -> Json<Value> {
    match crate::services::conntrack::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all tracked connections.
#[utoipa::path(
    get,
    path = "/api/conntrack/connections",
    tag = "ConnTrack",
    responses(
        (status = 200, description = "Active connections", body = Value)
    )
)]
pub async fn get_conntrack_connections() -> Json<Value> {
    match crate::services::conntrack::list_connections() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get connection tracking statistics.
#[utoipa::path(
    get,
    path = "/api/conntrack/stats",
    tag = "ConnTrack",
    responses(
        (status = 200, description = "ConnTrack statistics", body = Value)
    )
)]
pub async fn get_conntrack_stats() -> Json<Value> {
    match crate::services::conntrack::get_stats() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get top talkers from connection tracking.
#[utoipa::path(
    get,
    path = "/api/conntrack/top",
    tag = "ConnTrack",
    responses(
        (status = 200, description = "Top talkers", body = Value)
    )
)]
pub async fn get_conntrack_top() -> Json<Value> {
    match crate::services::conntrack::get_top_talkers() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConntrackFilterRequest {
    pub ip: Option<String>,
    pub port: Option<u32>,
    pub protocol: Option<String>,
}

/// Filter connections by IP, port, or protocol.
#[utoipa::path(
    post,
    path = "/api/conntrack/filter",
    tag = "ConnTrack",
    request_body = ConntrackFilterRequest,
    responses(
        (status = 200, description = "Filtered connections", body = Value)
    )
)]
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

/// Get detailed NAT connection tracking information.
#[utoipa::path(
    get,
    path = "/api/conntrack/detail",
    tag = "ConnTrack",
    responses(
        (status = 200, description = "ConnTrack detail", body = Value)
    )
)]
pub async fn get_conntrack_detail() -> Json<Value> {
    match crate::services::conntrack::get_nat_detail() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Backup management handlers ─────────────────────────────────────

// ── Traffic control handlers (native Rust) ────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct TrafficLimitRequest {
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_traffic_direction")]
    pub direction: String,
}

fn default_traffic_direction() -> String {
    "both".to_string()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TrafficIpLimitRequest {
    pub ip: String,
    pub rate: u64,
    pub burst: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TrafficPriorityRequest {
    pub name: String,
    pub queue: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct TrafficIpDeleteRequest {
    pub ip: String,
}

/// Get traffic control status.
#[utoipa::path(
    get,
    path = "/api/traffic/status",
    tag = "Traffic Control",
    responses(
        (status = 200, description = "Traffic status", body = Value)
    )
)]
pub async fn get_traffic_status() -> Json<Value> {
    match crate::services::traffic::show_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Set bandwidth limit on an interface or IP.
/// Body: { "type": "interface"|"ip", "target": "..."|"...", ... }
#[derive(Debug, Deserialize, ToSchema)]
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

/// Set bandwidth limit on an interface or IP address.
#[utoipa::path(
    post,
    path = "/api/traffic/limit",
    tag = "Traffic Control",
    request_body = TrafficLimitBody,
    responses(
        (status = 200, description = "Limit applied", body = Value)
    )
)]
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

/// Remove bandwidth limit from an interface.
#[utoipa::path(
    delete,
    path = "/api/traffic/limit/interface/{iface}",
    tag = "Traffic Control",
    params(
        ("iface" = String, Path, description = "Interface name")
    ),
    responses(
        (status = 200, description = "Limit removed", body = Value)
    )
)]
pub async fn remove_traffic_interface_limit(Path(iface): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_interface_limit(&iface) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Remove bandwidth limit from an IP address.
#[utoipa::path(
    delete,
    path = "/api/traffic/limit/ip/{ip}",
    tag = "Traffic Control",
    params(
        ("ip" = String, Path, description = "IP address")
    ),
    responses(
        (status = 200, description = "Limit removed", body = Value)
    )
)]
pub async fn remove_traffic_ip_limit(Path(ip): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_ip_limit(&ip) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Set traffic priority queue assignment.
#[utoipa::path(
    post,
    path = "/api/traffic/priority",
    tag = "Traffic Control",
    request_body = TrafficPriorityRequest,
    responses(
        (status = 200, description = "Priority set", body = Value)
    )
)]
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

/// Define an application classification rule for traffic shaping.
#[utoipa::path(
    post,
    path = "/api/traffic/app-class",
    tag = "Traffic Control",
    request_body = TrafficAppClassRequest,
    responses(
        (status = 200, description = "App class defined", body = Value)
    )
)]
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

/// Remove an application classification rule.
#[utoipa::path(
    delete,
    path = "/api/traffic/app-class/{name}",
    tag = "Traffic Control",
    params(
        ("name" = String, Path, description = "App class name")
    ),
    responses(
        (status = 200, description = "App class removed", body = Value)
    )
)]
pub async fn remove_traffic_app_class(Path(name): Path<String>) -> Json<Value> {
    match crate::services::traffic::remove_app_class(&name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Load default traffic control rules.
#[utoipa::path(
    post,
    path = "/api/traffic/defaults",
    tag = "Traffic Control",
    responses(
        (status = 200, description = "Defaults loaded", body = Value)
    )
)]
pub async fn load_traffic_defaults() -> Json<Value> {
    match crate::services::traffic::load_defaults() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get traffic control statistics.
#[utoipa::path(
    get,
    path = "/api/traffic/stats",
    tag = "Traffic Control",
    responses(
        (status = 200, description = "Traffic stats", body = Value)
    )
)]
pub async fn get_traffic_stats() -> Json<Value> {
    match crate::services::traffic::get_stats() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Reset all traffic control rules to defaults.
#[utoipa::path(
    post,
    path = "/api/traffic/reset",
    tag = "Traffic Control",
    responses(
        (status = 200, description = "Traffic reset", body = Value)
    )
)]
pub async fn reset_traffic() -> Json<Value> {
    match crate::services::traffic::reset() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── VPN management handlers ────────────────────────────────────────

/// Get VPN subsystem status.
#[utoipa::path(
    get,
    path = "/api/vpn/status",
    tag = "VPN",
    responses(
        (status = 200, description = "VPN status", body = Value)
    )
)]
pub async fn get_vpn_status() -> Json<Value> {
    match crate::services::vpn::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all active VPN connections.
#[utoipa::path(
    get,
    path = "/api/vpn/connections",
    tag = "VPN",
    responses(
        (status = 200, description = "VPN connections", body = Value)
    )
)]
pub async fn get_vpn_connections() -> Json<Value> {
    match crate::services::vpn::list_connections() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VpnDownRequest {
    pub vpn_type: String,
    pub name: String,
}

/// Configure a WireGuard VPN tunnel.
#[utoipa::path(
    post,
    path = "/api/vpn/wireguard/config",
    tag = "VPN",
    request_body = crate::services::vpn::WireGuardConfigRequest,
    responses(
        (status = 200, description = "WireGuard configured", body = Value)
    )
)]
pub async fn configure_wireguard(Json(req): Json<crate::services::vpn::WireGuardConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_wireguard(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Configure an IPsec VPN tunnel.
#[utoipa::path(
    post,
    path = "/api/vpn/ipsec/config",
    tag = "VPN",
    request_body = crate::services::vpn::IpsecConfigRequest,
    responses(
        (status = 200, description = "IPsec configured", body = Value)
    )
)]
pub async fn configure_ipsec(Json(req): Json<crate::services::vpn::IpsecConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_ipsec(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Configure an OpenVPN tunnel.
#[utoipa::path(
    post,
    path = "/api/vpn/openvpn/config",
    tag = "VPN",
    request_body = crate::services::vpn::OpenVpnConfigRequest,
    responses(
        (status = 200, description = "OpenVPN configured", body = Value)
    )
)]
pub async fn configure_openvpn(Json(req): Json<crate::services::vpn::OpenVpnConfigRequest>) -> Json<Value> {
    match crate::services::vpn::configure_openvpn(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Bring down a VPN tunnel by type and name.
#[utoipa::path(
    post,
    path = "/api/vpn/down",
    tag = "VPN",
    request_body = VpnDownRequest,
    responses(
        (status = 200, description = "VPN tunnel brought down", body = Value)
    )
)]
pub async fn vpn_down(Json(req): Json<VpnDownRequest>) -> Json<Value> {
    match crate::services::vpn::bring_down(&req.vpn_type, &req.name) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── Network diagnostics handlers (native Rust) ──────────────────────

/// Get diagnostic tools availability status.
#[utoipa::path(
    get,
    path = "/api/diag/status",
    tag = "Diagnostics",
    responses(
        (status = 200, description = "Diagnostics status", body = Value)
    )
)]
pub async fn get_diag_status() -> Json<Value> {
    match crate::services::diag::get_status() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Perform a ping to a host.
#[utoipa::path(
    post,
    path = "/api/diag/ping",
    tag = "Diagnostics",
    request_body = crate::services::diag::PingRequest,
    responses(
        (status = 200, description = "Ping results", body = Value)
    )
)]
pub async fn diag_ping(Json(req): Json<crate::services::diag::PingRequest>) -> Json<Value> {
    match crate::services::diag::ping(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Perform a traceroute to a host.
#[utoipa::path(
    post,
    path = "/api/diag/traceroute",
    tag = "Diagnostics",
    request_body = crate::services::diag::TracerouteRequest,
    responses(
        (status = 200, description = "Traceroute results", body = Value)
    )
)]
pub async fn diag_traceroute(Json(req): Json<crate::services::diag::TracerouteRequest>) -> Json<Value> {
    match crate::services::diag::traceroute(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Perform a DNS lookup for a domain.
#[utoipa::path(
    post,
    path = "/api/diag/dns",
    tag = "Diagnostics",
    request_body = crate::services::diag::DnsRequest,
    responses(
        (status = 200, description = "DNS lookup results", body = Value)
    )
)]
pub async fn diag_dns(Json(req): Json<crate::services::diag::DnsRequest>) -> Json<Value> {
    match crate::services::diag::dns_lookup(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Scan ports on a host.
#[utoipa::path(
    post,
    path = "/api/diag/portscan",
    tag = "Diagnostics",
    request_body = crate::services::diag::PortScanRequest,
    responses(
        (status = 200, description = "Port scan results", body = Value)
    )
)]
pub async fn diag_portscan(Json(req): Json<crate::services::diag::PortScanRequest>) -> Json<Value> {
    match crate::services::diag::port_scan(req) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

// ── VyOS-style configuration management handlers ────────────────

/// Get the full configuration as a hierarchical tree.
#[utoipa::path(
    get,
    path = "/api/config/tree",
    tag = "Configuration",
    responses(
        (status = 200, description = "Configuration tree", body = Value)
    )
)]
pub async fn get_config_tree() -> Json<Value> {
    let tree = crate::services::config_cli::get_tree();
    Json(json!({
        "status": "ok",
        "tree": tree
    }))
}

/// Get the staged (uncommitted) configuration tree.
#[utoipa::path(
    get,
    path = "/api/config/staging",
    tag = "Configuration",
    responses(
        (status = 200, description = "Staged configuration", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigSetRequest {
    pub path: String,
    pub value: serde_json::Value,
}

/// Set a configuration value at a dot-separated path (staged).
#[utoipa::path(
    post,
    path = "/api/config/set",
    tag = "Configuration",
    request_body = ConfigSetRequest,
    responses(
        (status = 200, description = "Value set", body = Value)
    )
)]
pub async fn config_set_value(Json(req): Json<ConfigSetRequest>) -> Json<Value> {
    match crate::services::config_cli::set_value(&req.path, req.value) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigDeleteRequest {
    pub path: String,
}

/// Delete a configuration value at a dot-separated path (staged).
#[utoipa::path(
    post,
    path = "/api/config/delete",
    tag = "Configuration",
    request_body = ConfigDeleteRequest,
    responses(
        (status = 200, description = "Value deleted", body = Value)
    )
)]
pub async fn config_delete_value(Json(req): Json<ConfigDeleteRequest>) -> Json<Value> {
    match crate::services::config_cli::delete_value(&req.path) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Commit all staged configuration changes.
#[utoipa::path(
    post,
    path = "/api/config/commit",
    tag = "Configuration",
    responses(
        (status = 200, description = "Changes committed", body = Value)
    )
)]
pub async fn config_commit() -> Json<Value> {
    match crate::services::config_cli::commit() {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Rollback configuration to a specific version.
#[utoipa::path(
    post,
    path = "/api/config/rollback/{version}",
    tag = "Configuration",
    params(
        ("version" = String, Path, description = "Configuration version")
    ),
    responses(
        (status = 200, description = "Rollback applied", body = Value)
    )
)]
pub async fn config_rollback(Path(version): Path<String>) -> Json<Value> {
    match crate::services::config_cli::rollback(&version) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Discard staged (uncommitted) changes.
#[utoipa::path(
    post,
    path = "/api/config/discard",
    tag = "Configuration",
    responses(
        (status = 200, description = "Changes discarded", body = Value)
    )
)]
pub async fn config_discard() -> Json<Value> {
    let result = crate::services::config_cli::execute_cli("", "discard");
    Json(json!(result))
}

/// Get diff between committed and staged configuration.
#[utoipa::path(
    get,
    path = "/api/config/diff",
    tag = "Configuration",
    responses(
        (status = 200, description = "Configuration diff", body = Value)
    )
)]
pub async fn config_diff() -> Json<Value> {
    let diff = crate::services::config_cli::get_diff();
    Json(json!({
        "status": "ok",
        "changes": diff.len(),
        "diff": diff
    }))
}

/// Get diff between two specific config versions.
#[utoipa::path(
    get,
    path = "/api/config/diff/{v1}/{v2}",
    tag = "Configuration",
    params(
        ("v1" = String, Path, description = "First version"),
        ("v2" = String, Path, description = "Second version")
    ),
    responses(
        (status = 200, description = "Version diff", body = Value)
    )
)]
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

/// Get configuration version history.
#[utoipa::path(
    get,
    path = "/api/config/history",
    tag = "Configuration",
    responses(
        (status = 200, description = "Config history", body = Value)
    )
)]
pub async fn config_history() -> Json<Value> {
    let history = crate::services::config_cli::list_history();
    Json(json!({
        "status": "ok",
        "history": history,
        "count": history.len()
    }))
}

/// List saved configuration templates.
#[utoipa::path(
    get,
    path = "/api/config/templates",
    tag = "Configuration",
    responses(
        (status = 200, description = "Template list", body = Value)
    )
)]
pub async fn config_list_templates() -> Json<Value> {
    let templates = crate::services::config_cli::list_templates();
    Json(json!({
        "status": "ok",
        "templates": templates,
        "count": templates.len()
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Save current configuration as a named template.
#[utoipa::path(
    post,
    path = "/api/config/template/save",
    tag = "Configuration",
    request_body = SaveTemplateRequest,
    responses(
        (status = 200, description = "Template saved", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApplyTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

/// Apply a named template (with optional variable substitution) to staging.
#[utoipa::path(
    post,
    path = "/api/config/template/apply",
    tag = "Configuration",
    request_body = ApplyTemplateRequest,
    responses(
        (status = 200, description = "Template applied", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CliSessionRequest {
    #[serde(default)]
    pub command: Option<String>,
}

/// Create a new CLI session or execute a command in an existing session.
#[utoipa::path(
    post,
    path = "/api/config/cli/session",
    tag = "Configuration",
    request_body = CliSessionRequest,
    responses(
        (status = 200, description = "CLI session", body = Value)
    )
)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CliExecuteRequest {
    pub session_id: String,
    pub command: String,
}

/// Execute a CLI command in an existing session.
#[utoipa::path(
    post,
    path = "/api/config/cli/execute",
    tag = "Configuration",
    request_body = CliExecuteRequest,
    responses(
        (status = 200, description = "Command result", body = Value)
    )
)]
pub async fn config_cli_execute(Json(req): Json<CliExecuteRequest>) -> Json<Value> {
    let result = crate::services::config_cli::execute_cli(&req.session_id, &req.command);
    Json(json!(result))
}

// ── Config Import/Export handlers ──────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigExportRequest {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigImportRequest {
    /// The full export data as JSON string
    pub export_json: String,
    /// Sections to import (empty = all)
    #[serde(default)]
    pub sections: Vec<String>,
    /// Whether to overwrite existing values (default true)
    #[serde(default = "default_true_bool")]
    pub overwrite: bool,
    /// Whether to commit immediately
    #[serde(default)]
    pub auto_commit: bool,
    /// Description for the import
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigValidateRequest {
    /// The full export data as JSON string
    pub export_json: String,
    /// Sections to validate (empty = all)
    #[serde(default)]
    pub sections: Vec<String>,
}

/// Export the current configuration as a downloadable JSON/TOML file.
#[utoipa::path(
    get,
    path = "/api/config/export",
    tag = "Configuration",
    params(
        ("hostname" = Option<String>, Query, description = "Hostname for the export"),
        ("description" = Option<String>, Query, description = "Description for the export"),
        ("format" = Option<String>, Query, description = "Export format: json or toml")
    ),
    responses(
        (status = 200, description = "Exported configuration", body = Value)
    )
)]
pub async fn config_export(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let hostname = params.get("hostname").map(|s| s.as_str());
    let description = params.get("description").map(|s| s.as_str());
    let format = params.get("format").map(|s| s.as_str()).unwrap_or("json");

    let result = match format {
        "toml" => crate::services::config_io::export_as_toml(hostname, description)
            .map(|s| json!({ "status": "ok", "format": "toml", "data": s })),
        _ => crate::services::config_io::export_as_json(hostname, description)
            .map(|s| json!({ "status": "ok", "format": "json", "data": s })),
    };

    match result {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Import configuration from a JSON export file.
#[utoipa::path(
    post,
    path = "/api/config/import",
    tag = "Configuration",
    request_body = ConfigImportRequest,
    responses(
        (status = 200, description = "Import result", body = Value)
    )
)]
pub async fn config_import(Json(req): Json<ConfigImportRequest>) -> Json<Value> {
    match crate::services::config_io::api_import_config(
        &req.export_json,
        req.sections,
        req.overwrite,
        req.auto_commit,
    ) {
        Ok(result) => Json(result),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Validate a config import before applying it.
#[utoipa::path(
    post,
    path = "/api/config/validate",
    tag = "Configuration",
    request_body = ConfigValidateRequest,
    responses(
        (status = 200, description = "Validation result", body = Value)
    )
)]
pub async fn config_validate(Json(req): Json<ConfigValidateRequest>) -> Json<Value> {
    match crate::services::config_io::api_validate_import(&req.export_json, &req.sections) {
        Ok(result) => Json(json!({
            "status": "ok",
            "validation": result
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Get the config import history.
#[utoipa::path(
    get,
    path = "/api/config/import/history",
    tag = "Configuration",
    params(
        ("limit" = Option<i64>, Query, description = "Max number of history entries")
    ),
    responses(
        (status = 200, description = "Import history", body = Value)
    )
)]
pub async fn config_import_history(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let limit = params.get("limit")
        .and_then(|s| s.parse::<i64>().ok());

    match crate::services::config_io::api_import_history(limit) {
        Ok(history) => Json(json!({
            "status": "ok",
            "history": history,
            "count": history.len()
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

// ── Service Manager handlers ─────────────────────────────────────────

/// List all managed services with their current status.
#[utoipa::path(
    get,
    path = "/api/services",
    tag = "Services",
    responses(
        (status = 200, description = "List of services", body = Value)
    )
)]
pub async fn list_services(State(state): State<Arc<AppState>>) -> Json<Value> {
    let services = state.service_manager.list_services().await;
    Json(json!({
        "status": "ok",
        "services": services,
        "count": services.len(),
    }))
}

/// Get status of a single service.
#[utoipa::path(
    get,
    path = "/api/services/{name}/status",
    tag = "Services",
    params(
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Service status", body = Value)
    )
)]
pub async fn get_service_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Value> {
    match state.service_manager.status(&name).await {
        Ok(info) => Json(json!({ "status": "ok", "service": info })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Start a service by name.
#[utoipa::path(
    post,
    path = "/api/services/{name}/start",
    tag = "Services",
    params(
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Service started", body = Value)
    )
)]
pub async fn start_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Value> {
    match state.service_manager.start_service(&name).await {
        Ok(info) => Json(json!({ "status": "ok", "service": info })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Stop a service by name.
#[utoipa::path(
    post,
    path = "/api/services/{name}/stop",
    tag = "Services",
    params(
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Service stopped", body = Value)
    )
)]
pub async fn stop_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Value> {
    match state.service_manager.stop_service(&name).await {
        Ok(info) => Json(json!({ "status": "ok", "service": info })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Restart a service with automatic rollback on failure.
#[utoipa::path(
    post,
    path = "/api/services/{name}/restart",
    tag = "Services",
    params(
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Service restarted", body = Value)
    )
)]
pub async fn restart_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Value> {
    match state.service_manager.restart_service(&name).await {
        Ok(info) => Json(json!({ "status": "ok", "service": info })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Hot-reload service configuration.
#[utoipa::path(
    post,
    path = "/api/services/{name}/reload",
    tag = "Services",
    params(
        ("name" = String, Path, description = "Service name")
    ),
    responses(
        (status = 200, description = "Service reloaded", body = Value)
    )
)]
pub async fn reload_service(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Value> {
    match state.service_manager.reload_service(&name).await {
        Ok(info) => Json(json!({ "status": "ok", "service": info })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

