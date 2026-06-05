use axum::extract::{State, Path, Query};
use axum::Json;
use axum::http::{HeaderMap, header};
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;
use utoipa::ToSchema;
use crate::api::AppState;
use crate::auth::LoginRequest;
use crate::security;

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

/// Authenticate and obtain a JWT token with CSRF protection.
///
/// On successful login, returns a JWT token and a CSRF token.
/// The client must include the CSRF token in the `X-CSRF-Token` header
/// for all subsequent state-changing requests (POST, PUT, DELETE, PATCH).
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
pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Json<Value> {
    let client_ip = security::rate_limit::extract_client_ip(&headers).to_string();

    // Validate username format
    if let Err(e) = security::validation::validate_username(&req.username) {
        security::audit::log_audit_event(
            &req.username,
            security::audit::AuditAction::LoginFailure,
            Some("POST"),
            Some("/api/auth/login"),
            Some(&client_ip),
            Some(401),
            Some(&e),
        );
        return Json(json!({
            "success": false,
            "error": { "code": "INVALID_CREDENTIALS", "message": "Invalid username or password" }
        }));
    }

    if crate::auth::verify_credentials(&req.username, &req.password) {
        match crate::auth::generate_token(&req.username) {
            Ok(token) => {
                // Generate CSRF token
                let csrf_token = state.csrf_state.generate_token(&req.username).await;

                // Create session
                let user_agent = headers
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown");
                let session = state.session_manager.create_session(
                    &req.username,
                    &client_ip,
                    user_agent,
                    &csrf_token,
                ).await;

                // Audit log success
                security::audit::log_audit_event(
                    &req.username,
                    security::audit::AuditAction::LoginSuccess,
                    Some("POST"),
                    Some("/api/auth/login"),
                    Some(&client_ip),
                    Some(200),
                    Some(&format!("Session: {}", session.session_id)),
                );

                Json(json!({
                    "success": true,
                    "data": {
                        "token": token,
                        "csrf_token": csrf_token,
                        "session_id": session.session_id,
                        "expires_in": 86400
                    }
                }))
            }
            Err(e) => {
                security::audit::log_audit_event(
                    &req.username,
                    security::audit::AuditAction::LoginFailure,
                    Some("POST"),
                    Some("/api/auth/login"),
                    Some(&client_ip),
                    Some(500),
                    Some(&format!("Token generation error: {}", e)),
                );
                Json(json!({
                    "success": false,
                    "error": { "code": "TOKEN_ERROR", "message": e.to_string() }
                }))
            }
        }
    } else {
        // Audit log failure
        security::audit::log_audit_event(
            &req.username,
            security::audit::AuditAction::LoginFailure,
            Some("POST"),
            Some("/api/auth/login"),
            Some(&client_ip),
            Some(401),
            Some("Invalid credentials"),
        );

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
    match tokio::task::spawn_blocking(|| crate::vpp::native::get_interfaces()).await {
        Ok(Ok(interfaces)) => Json(json!({ "interfaces": interfaces })),
        Ok(Err(e)) => Json(json!({ "error": e.to_string() })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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
    let iface_name = name.clone();
    match tokio::task::spawn_blocking(move || crate::vpp::native::set_interface_state(&iface_name, "up")).await {
        Ok(Ok(())) => Json(json!({ "status": "ok", "message": format!("Interface {} set to up", name) })),
        Ok(Err(e)) => Json(json!({ "error": e.to_string() })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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
    let iface_name = name.clone();
    match tokio::task::spawn_blocking(move || crate::vpp::native::set_interface_state(&iface_name, "down")).await {
        Ok(Ok(())) => Json(json!({ "status": "ok", "message": format!("Interface {} set to down", name) })),
        Ok(Err(e)) => Json(json!({ "error": e.to_string() })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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
    // Input validation
    let mut validation_errors: Vec<String> = Vec::new();
    if let Err(e) = security::validation::validate_interface_name(&name, "interface") {
        validation_errors.push(e);
    }
    if let Some(mtu) = req.mtu {
        if let Err(e) = security::validation::validate_mtu(mtu, "mtu") {
            validation_errors.push(e);
        }
    }
    if let Some(ref ip) = req.ip_add {
        if let Err(e) = security::validation::validate_ip_or_cidr(ip, "ip_add") {
            validation_errors.push(e);
        }
    }
    if let Some(ref ip) = req.ip_remove {
        if let Err(e) = security::validation::validate_ip_or_cidr(ip, "ip_remove") {
            validation_errors.push(e);
        }
    }
    if !validation_errors.is_empty() {
        return Json(json!({
            "success": false,
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Input validation failed",
                "details": validation_errors
            }
        }));
    }

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
    match tokio::task::spawn_blocking(move || crate::vpp::native::get_interface_stats(&name)).await {
        Ok(Ok(stats)) => Json(json!({ "stats": stats })),
        Ok(Err(e)) => Json(json!({ "error": e.to_string() })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
    }
}

// ── Interface Binding handlers ──────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct BindInterfaceRequest {
    /// Linux VF interface name (e.g. "enp1s0")
    pub vf_name: String,
    /// VPP interface name (e.g. "wan0")
    pub vpp_name: String,
    /// Binding method: "rdma" (default) or "dpdk"
    #[serde(default = "default_bind_method")]
    pub method: String,
    /// PCI address (optional, auto-detected if not provided)
    pub pci: Option<String>,
}

fn default_bind_method() -> String {
    "rdma".to_string()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UnbindInterfaceRequest {
    /// VPP interface name to unbind (e.g. "wan0")
    pub vpp_name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigureBoundRequest {
    /// IP address with CIDR (e.g. "192.168.1.1/24")
    pub ip: Option<String>,
    /// MTU value
    pub mtu: Option<u32>,
}

/// Bind a VF interface to VPP.
///
/// Creates a VPP interface from a physical VF using RDMA host-interface (default)
/// or DPDK driver binding.
#[utoipa::path(
    post,
    path = "/api/interfaces/bind",
    tag = "Interfaces",
    request_body = BindInterfaceRequest,
    responses(
        (status = 200, description = "Interface bound", body = Value),
        (status = 400, description = "Bind error", body = Value)
    )
)]
pub async fn bind_interface(Json(req): Json<BindInterfaceRequest>) -> Json<Value> {
    match crate::vpp::native::bind_interface(
        &req.vf_name,
        &req.vpp_name,
        &req.method,
        req.pci.as_deref(),
    ) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Unbind an interface from VPP.
///
/// Removes the VPP interface and restores the VF to the kernel driver.
#[utoipa::path(
    post,
    path = "/api/interfaces/unbind",
    tag = "Interfaces",
    request_body = UnbindInterfaceRequest,
    responses(
        (status = 200, description = "Interface unbound", body = Value),
        (status = 400, description = "Unbind error", body = Value)
    )
)]
pub async fn unbind_interface(Json(req): Json<UnbindInterfaceRequest>) -> Json<Value> {
    match crate::vpp::native::unbind_interface(&req.vpp_name) {
        Ok(result) => Json(json!(result)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// List all VF interfaces bound to VPP and available unbound VFs.
///
/// Returns both currently bound interfaces with their binding metadata
/// and VF interfaces that exist on the system but are not yet bound.
#[utoipa::path(
    get,
    path = "/api/interfaces/bound",
    tag = "Interfaces",
    responses(
        (status = 200, description = "Bound interfaces list", body = Value)
    )
)]
pub async fn list_bound_interfaces() -> Json<Value> {
    match crate::vpp::native::list_bound_interfaces() {
        Ok(list) => Json(json!(list)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Configure a bound interface (IP, MTU, bring up).
#[utoipa::path(
    post,
    path = "/api/interfaces/{name}/configure-bound",
    tag = "Interfaces",
    params(
        ("name" = String, Path, description = "VPP interface name")
    ),
    request_body = ConfigureBoundRequest,
    responses(
        (status = 200, description = "Interface configured", body = Value)
    )
)]
pub async fn configure_bound_interface(
    Path(name): Path<String>,
    Json(req): Json<ConfigureBoundRequest>,
) -> Json<Value> {
    match crate::vpp::native::configure_bound_interface(
        &name,
        req.ip.as_deref(),
        req.mtu,
    ) {
        Ok(data) => Json(data),
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
    match tokio::task::spawn_blocking(|| run_vpp_cmd("dump", &[])).await {
        Ok(Ok(data)) => Json(json!({ "clients": data })),
        Ok(Err(e)) => Json(json!({ "error": e })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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
    // Use Python script for PPPoE status
    let result = std::process::Command::new("python3")
        .arg("/root/VectorOS/vpp-tools/pppoe_manager.py")
        .arg("status")
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                match serde_json::from_str::<Value>(&stdout) {
                    Ok(data) => Json(data),
                    Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Json(json!({ "error": stderr.to_string() }))
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
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
    // Input validation
    let mut errors: Vec<String> = Vec::new();

    if let Err(e) = security::validation::validate_non_empty_string(&config.username, "username", 128) {
        errors.push(e);
    }
    if let Err(e) = security::validation::validate_non_empty_string(&config.password, "password", 128) {
        errors.push(e);
    }
    if let Err(e) = security::validation::validate_interface_name(&config.interface, "interface") {
        errors.push(e);
    }
    if let Err(e) = security::validation::validate_mtu(config.mtu, "mtu") {
        errors.push(e);
    }
    if let Err(e) = security::validation::validate_mru(config.mru, "mru") {
        errors.push(e);
    }

    if !errors.is_empty() {
        return Json(json!({
            "success": false,
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Input validation failed",
                "details": errors
            }
        }));
    }

    // Map interface name to sw_if_index
    let sw_if_index = match config.interface.as_str() {
        "enp1s0" => "1",  // wan0
        "enp2s0" => "2",  // lan0
        "enp3s0" => "3",  // lan1
        "wan0" => "1",
        "lan0" => "2",
        "lan1" => "3",
        _ => return Json(json!({ "error": format!("Unknown interface: {}", config.interface) })),
    };

    // Use Python PPPoE manager
    let result = std::process::Command::new("python3")
        .arg("/root/VectorOS/vpp-tools/pppoe_manager.py")
        .arg("create")
        .arg("--sw-if-index").arg(sw_if_index)
        .arg("--username").arg(&config.username)
        .arg("--password").arg(&config.password)
        .arg("--mtu").arg(config.mtu.to_string())
        .arg("--mru").arg(config.mru.to_string())
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                match serde_json::from_str::<Value>(&stdout) {
                    Ok(data) => Json(data),
                    Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Json(json!({ "error": stderr.to_string() }))
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
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
    // Use VPP CLI to get NAT status
    let output = std::process::Command::new("vppctl")
        .args(["show", "nat44", "ei", "interfaces"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut interfaces = Vec::new();

            for line in stdout.lines() {
                let line = line.trim();
                if line.contains(" in") || line.contains(" out") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        interfaces.push(serde_json::json!({
                            "name": parts[0],
                            "direction": parts[1]
                        }));
                    }
                }
            }

            Json(json!({
                "enabled": !interfaces.is_empty(),
                "interfaces": interfaces,
                "session_count": 0
            }))
        }
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
    // Use VPP CLI directly for NAT configuration
    let result = std::process::Command::new("vppctl")
        .args(["nat44", "ei", "plugin", "enable", "sessions", "65536", "users", "8192"])
        .output();

    // Configure NAT interfaces
    let _ = std::process::Command::new("vppctl")
        .args(["set", "interface", "nat44", "ei", "in", "lan0", "out", "pppoe-wan0"])
        .output();

    let _ = std::process::Command::new("vppctl")
        .args(["nat44", "ei", "add", "interface", "address", "pppoe-wan0"])
        .output();

    Json(json!({ "status": "ok", "message": "NAT enabled" }))
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
    // Run both expensive calls in parallel on the blocking thread pool
    let system_info_handle = tokio::task::spawn_blocking(|| crate::vpp::native::get_system_info());
    let vpp_perf_handle = tokio::task::spawn_blocking(|| crate::vpp::native::get_vpp_performance());

    let system_info = system_info_handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
    let vpp_perf = vpp_perf_handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!(e)));

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
    match tokio::task::spawn_blocking(|| crate::vpp::native::get_vpp_performance()).await {
        Ok(Ok(perf)) => Json(json!({
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
        Ok(Err(e)) => Json(json!({ "error": e.to_string() })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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
    match tokio::task::spawn_blocking(|| {
        let mut cmd = std::process::Command::new("python3");
        cmd.arg("/root/VectorOS/vpp-tools/config_manager.py");
        cmd.arg("get");
        cmd.output()
    }).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Ok(Err(e)) => Json(json!({ "error": format!("Command error: {}", e) })),
        Err(e) => Json(json!({ "error": format!("Task join error: {}", e) })),
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

/// Disable the DNS resolver.
#[utoipa::path(
    post,
    path = "/api/dns/disable",
    tag = "DNS",
    responses(
        (status = 200, description = "DNS disabled", body = Value)
    )
)]
pub async fn disable_dns() -> Json<Value> {
    match crate::services::dns::disable() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Clear the DNS cache.
#[utoipa::path(
    post,
    path = "/api/dns/cache/clear",
    tag = "DNS",
    responses(
        (status = 200, description = "Cache cleared", body = Value)
    )
)]
pub async fn clear_dns_cache() -> Json<Value> {
    match crate::services::dns::clear_cache() {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Get DNS cache statistics.
#[utoipa::path(
    get,
    path = "/api/dns/cache/stats",
    tag = "DNS",
    responses(
        (status = 200, description = "Cache statistics", body = Value)
    )
)]
pub async fn get_dns_cache_stats() -> Json<Value> {
    match crate::services::dns::get_cache_stats() {
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
    // Validate prefix (CIDR format)
    if let Err(e) = security::validation::validate_cidr(&req.prefix, "prefix") {
        return Json(json!({
            "success": false,
            "error": { "code": "VALIDATION_ERROR", "message": e }
        }));
    }
    // Validate nexthop if provided
    if let Some(ref nexthop) = req.nexthop {
        if let Err(e) = security::validation::validate_ip_address(nexthop, "nexthop") {
            return Json(json!({
                "success": false,
                "error": { "code": "VALIDATION_ERROR", "message": e }
            }));
        }
    }
    // Validate interface name if provided
    if let Some(ref iface) = req.interface {
        if let Err(e) = security::validation::validate_interface_name(iface, "interface") {
            return Json(json!({
                "success": false,
                "error": { "code": "VALIDATION_ERROR", "message": e }
            }));
        }
    }
    // Validate distance if provided
    if let Some(distance) = req.distance {
        if distance > 255 {
            return Json(json!({
                "success": false,
                "error": { "code": "VALIDATION_ERROR", "message": "distance must be 0-255" }
            }));
        }
    }

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

/// Disable the DHCP server.
#[utoipa::path(
    post,
    path = "/api/dhcp/disable",
    tag = "DHCP",
    responses(
        (status = 200, description = "DHCP disabled", body = Value)
    )
)]
pub async fn disable_dhcp() -> Json<Value> {
    match crate::services::dhcp::disable() {
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
    // Input validation for firewall rule
    let mut errors: Vec<String> = Vec::new();

    if req.action != "accept" && req.action != "reject" && req.action != "drop" && req.action != "log" {
        errors.push(format!("action: must be accept, reject, drop, or log, got '{}'", req.action));
    }
    if let Some(ref src_ip) = req.src_ip {
        if let Err(e) = security::validation::validate_ip_or_cidr(src_ip, "src_ip") {
            errors.push(e);
        }
    }
    if let Some(ref dst_ip) = req.dst_ip {
        if let Err(e) = security::validation::validate_ip_or_cidr(dst_ip, "dst_ip") {
            errors.push(e);
        }
    }
    if let Some(ref src_port) = req.src_port {
        if let Err(e) = security::validation::validate_port_string(src_port, "src_port") {
            errors.push(e);
        }
    }
    if let Some(ref dst_port) = req.dst_port {
        if let Err(e) = security::validation::validate_port_string(dst_port, "dst_port") {
            errors.push(e);
        }
    }
    if let Some(ref desc) = req.description {
        if let Err(e) = security::validation::validate_description(desc, "description") {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Json(json!({
            "success": false,
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Input validation failed",
                "details": errors
            }
        }));
    }

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

// ---------------------------------------------------------------------------
// PPPoE Auto-Connect handlers
// ---------------------------------------------------------------------------

/// Request body for starting auto-connect.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PppoeAutoConnectStartRequest {
    /// PPPoE username (optional, uses config default if empty).
    #[serde(default)]
    pub username: String,
    /// PPPoE password (optional, uses config default if empty).
    #[serde(default)]
    pub password: String,
}

/// Request body for configuring auto-connect.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PppoeAutoConnectConfigRequest {
    /// Enable/disable auto-connect.
    pub enabled: Option<bool>,
    /// Maximum retries before giving up. 0 = infinite.
    pub max_retries: Option<u32>,
    /// Initial retry interval in seconds.
    pub retry_interval: Option<u64>,
    /// Exponential backoff multiplier.
    pub backoff_factor: Option<f64>,
    /// Maximum retry interval cap in seconds.
    pub max_retry_interval: Option<u64>,
    /// Interval between connection status checks in seconds.
    pub check_interval: Option<u64>,
    /// Interval between health checks while connected in seconds.
    pub health_check_interval: Option<u64>,
}

/// Start PPPoE auto-connect.
#[utoipa::path(
    post,
    path = "/api/pppoe/autoconnect/start",
    tag = "PPPoE",
    request_body = PppoeAutoConnectStartRequest,
    responses(
        (status = 200, description = "Auto-connect started", body = Value)
    )
)]
pub async fn start_pppoe_autoconnect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PppoeAutoConnectStartRequest>,
) -> Json<Value> {
    // Get credentials from request or fall back to config
    let pppoe = state.config.network.pppoe.as_ref();
    let username = if req.username.is_empty() {
        pppoe.map(|p| p.username.clone()).unwrap_or_default()
    } else {
        req.username
    };
    let password = if req.password.is_empty() {
        pppoe.map(|p| p.password.clone()).unwrap_or_default()
    } else {
        req.password
    };

    // Enable the service
    {
        let mut config = state.pppoe_auto.config_for_write().await;
        config.enabled = true;
    }

    match state.pppoe_auto.start(username, password).await {
        Ok(()) => {
            let status = state.pppoe_auto.get_status().await;
            Json(json!({ "status": "ok", "message": "Auto-connect started", "autoconnect": status }))
        }
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Stop PPPoE auto-connect.
#[utoipa::path(
    post,
    path = "/api/pppoe/autoconnect/stop",
    tag = "PPPoE",
    responses(
        (status = 200, description = "Auto-connect stopped", body = Value)
    )
)]
pub async fn stop_pppoe_autoconnect(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    {
        let mut config = state.pppoe_auto.config_for_write().await;
        config.enabled = false;
    }

    match state.pppoe_auto.stop().await {
        Ok(()) => {
            let status = state.pppoe_auto.get_status().await;
            Json(json!({ "status": "ok", "message": "Auto-connect stopped", "autoconnect": status }))
        }
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Get PPPoE auto-connect status.
#[utoipa::path(
    get,
    path = "/api/pppoe/autoconnect/status",
    tag = "PPPoE",
    responses(
        (status = 200, description = "Auto-connect status", body = Value)
    )
)]
pub async fn get_pppoe_autoconnect_status(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let status = state.pppoe_auto.get_status().await;
    Json(json!(status))
}

/// Configure PPPoE auto-connect parameters.
#[utoipa::path(
    post,
    path = "/api/pppoe/autoconnect/config",
    tag = "PPPoE",
    request_body = PppoeAutoConnectConfigRequest,
    responses(
        (status = 200, description = "Auto-connect configured", body = Value)
    )
)]
pub async fn configure_pppoe_autoconnect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PppoeAutoConnectConfigRequest>,
) -> Json<Value> {
    {
        let mut config = state.pppoe_auto.config_for_write().await;
        if let Some(enabled) = req.enabled {
            config.enabled = enabled;
        }
        if let Some(max_retries) = req.max_retries {
            config.max_retries = max_retries;
        }
        if let Some(retry_interval) = req.retry_interval {
            config.retry_interval = retry_interval;
        }
        if let Some(backoff_factor) = req.backoff_factor {
            config.backoff_factor = backoff_factor;
        }
        if let Some(max_retry_interval) = req.max_retry_interval {
            config.max_retry_interval = max_retry_interval;
        }
        if let Some(check_interval) = req.check_interval {
            config.check_interval = check_interval;
        }
        if let Some(health_check_interval) = req.health_check_interval {
            config.health_check_interval = health_check_interval;
        }
    }

    state.pppoe_auto.update_config(state.pppoe_auto.get_config().await).await;
    let status = state.pppoe_auto.get_status().await;
    Json(json!({ "status": "ok", "message": "Auto-connect configured", "autoconnect": status }))
}

// ── Monitoring handlers ──────────────────────────────────────────────

/// Get current system metrics.
#[utoipa::path(
    get,
    path = "/api/monitor/metrics",
    tag = "Monitoring",
    responses(
        (status = 200, description = "Current system metrics", body = Value)
    )
)]
pub async fn get_monitor_metrics() -> Json<Value> {
    match crate::services::monitor::get_current_metrics() {
        Ok(Some(metrics)) => {
            let health_score = crate::services::monitor::compute_health_score(&metrics);
            Json(json!({
                "status": "ok",
                "health_score": health_score,
                "metrics": metrics
            }))
        }
        Ok(None) => Json(json!({
            "status": "ok",
            "health_score": 0,
            "metrics": null,
            "message": "No metrics collected yet"
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Get historical metrics for the last N hours.
#[utoipa::path(
    get,
    path = "/api/monitor/history",
    tag = "Monitoring",
    params(
        ("hours" = Option<u32>, Query, description = "Hours of history (default 1)"),
        ("limit" = Option<u32>, Query, description = "Max data points (default 200)")
    ),
    responses(
        (status = 200, description = "Historical metrics", body = Value)
    )
)]
pub async fn get_monitor_history(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let hours = params.get("hours")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let limit = params.get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(200);

    match crate::services::monitor::get_history(hours, limit) {
        Ok(history) => Json(json!({
            "status": "ok",
            "hours": hours,
            "count": history.len(),
            "history": history
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Get active (unacknowledged) alerts.
#[utoipa::path(
    get,
    path = "/api/monitor/alerts",
    tag = "Monitoring",
    responses(
        (status = 200, description = "Active alerts", body = Value)
    )
)]
pub async fn get_monitor_alerts() -> Json<Value> {
    match crate::services::monitor::get_active_alerts() {
        Ok(alerts) => Json(json!({
            "status": "ok",
            "count": alerts.len(),
            "alerts": alerts
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

/// Acknowledge an alert.
#[utoipa::path(
    post,
    path = "/api/monitor/alerts/ack",
    tag = "Monitoring",
    request_body = crate::services::monitor::AlertAckRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = Value)
    )
)]
pub async fn acknowledge_alert(
    Json(req): Json<crate::services::monitor::AlertAckRequest>,
) -> Json<Value> {
    match crate::services::monitor::acknowledge_alert(req.alert_id, &req.acked_by) {
        Ok(true) => Json(json!({
            "status": "ok",
            "message": format!("Alert {} acknowledged", req.alert_id)
        })),
        Ok(false) => Json(json!({
            "status": "error",
            "error": format!("Alert {} not found or already acknowledged", req.alert_id)
        })),
        Err(e) => Json(json!({ "status": "error", "error": e.to_string() })),
    }
}

// ── Security management handlers ───────────────────────────────────────

/// Logout and invalidate the current session.
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "Authentication",
    responses(
        (status = 200, description = "Logged out", body = Value)
    )
)]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<Value> {
    let client_ip = security::rate_limit::extract_client_ip(&headers).to_string();

    // Extract user from auth header (token already validated by auth middleware)
    let user = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| crate::auth::validate_token(token).ok())
        .map(|claims| claims.sub)
        .unwrap_or_else(|| "unknown".to_string());

    // Remove CSRF token
    state.csrf_state.remove_token(&user).await;

    // Invalidate all sessions for this user
    state.session_manager.invalidate_all_user_sessions(&user).await;

    // Audit log
    security::audit::log_audit_event(
        &user,
        security::audit::AuditAction::Logout,
        Some("POST"),
        Some("/api/auth/logout"),
        Some(&client_ip),
        Some(200),
        None,
    );

    Json(json!({
        "success": true,
        "message": "Logged out successfully"
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

/// Change the current user's password.
#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    tag = "Authentication",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed", body = Value),
        (status = 400, description = "Validation error", body = Value),
        (status = 401, description = "Invalid current password", body = Value)
    )
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Json<Value> {
    let client_ip = security::rate_limit::extract_client_ip(&headers).to_string();

    let user = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| crate::auth::validate_token(token).ok())
        .map(|claims| claims.sub)
        .unwrap_or_else(|| "unknown".to_string());

    // Validate new password
    if let Err(e) = security::validation::validate_password(&req.new_password) {
        return Json(json!({
            "success": false,
            "error": { "code": "VALIDATION_ERROR", "message": e }
        }));
    }

    match crate::auth::change_password(&user, &req.old_password, &req.new_password) {
        Ok(()) => {
            // Invalidate all existing sessions (user must re-login)
            state.session_manager.invalidate_all_user_sessions(&user).await;
            state.csrf_state.remove_token(&user).await;

            Json(json!({
                "success": true,
                "message": "Password changed successfully. Please log in again."
            }))
        }
        Err(e) => {
            security::audit::log_audit_event(
                &user,
                security::audit::AuditAction::LoginFailure,
                Some("POST"),
                Some("/api/auth/change-password"),
                Some(&client_ip),
                Some(401),
                Some(&e),
            );
            Json(json!({
                "success": false,
                "error": { "code": "PASSWORD_CHANGE_FAILED", "message": e }
            }))
        }
    }
}

/// Query audit logs for security events.
#[utoipa::path(
    get,
    path = "/api/security/audit-logs",
    tag = "Security",
    params(
        ("limit" = Option<i64>, Query, description = "Max entries (default 100)")
    ),
    responses(
        (status = 200, description = "Audit log entries", body = Value)
    )
)]
pub async fn get_audit_logs(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .min(1000); // Cap at 1000

    match security::audit::get_audit_logs(limit) {
        Ok(entries) => Json(json!({
            "success": true,
            "data": {
                "entries": entries,
                "count": entries.len()
            }
        })),
        Err(e) => Json(json!({
            "success": false,
            "error": { "code": "AUDIT_ERROR", "message": e }
        })),
    }
}

/// Get current security configuration and status.
#[utoipa::path(
    get,
    path = "/api/security/status",
    tag = "Security",
    responses(
        (status = 200, description = "Security status", body = Value)
    )
)]
pub async fn get_security_status() -> Json<Value> {
    Json(json!({
        "success": true,
        "data": {
            "rate_limiting": {
                "enabled": true,
                "login_limit": "10 requests/minute",
                "api_read_limit": "120 requests/minute",
                "api_write_limit": "30 requests/minute"
            },
            "csrf_protection": {
                "enabled": true,
                "header": "X-CSRF-Token",
                "description": "CSRF token required for all state-changing requests"
            },
            "security_headers": {
                "enabled": true,
                "headers": [
                    "X-Content-Type-Options: nosniff",
                    "X-Frame-Options: DENY",
                    "X-XSS-Protection: 1; mode=block",
                    "Referrer-Policy: strict-origin-when-cross-origin",
                    "Permissions-Policy: camera=(), microphone=(), geolocation=()",
                    "Cache-Control: no-store (API responses)",
                    "Content-Security-Policy: default-src 'none' (API responses)",
                    "Strict-Transport-Security: max-age=31536000"
                ]
            },
            "password_hashing": {
                "algorithm": "bcrypt",
                "cost_factor": 12
            },
            "audit_logging": {
                "enabled": true,
                "events_logged": [
                    "login_success",
                    "login_failure",
                    "logout",
                    "password_change",
                    "config_changes",
                    "service_operations",
                    "unauthorized_access"
                ]
            },
            "session_management": {
                "enabled": true,
                "max_sessions_per_user": 5,
                "session_duration_hours": 24,
                "ip_binding": false
            }
        }
    }))
}


/// Add VPP ACL rule
#[utoipa::path(
    post,
    path = "/api/vpp/acl/add",
    tag = "VPP ACL",
    request_body = VppAclRule,
    responses(
        (status = 200, description = "ACL rule added", body = Value)
    )
)]
pub async fn add_vpp_acl(
    Json(rule): Json<VppAclRule>,
) -> Json<Value> {
    let acl = crate::services::vpp_acl::VppAclManager::new();
    let acl_rule = crate::services::vpp_acl::AclRule {
        index: 0,
        action: rule.action,
        src: rule.src,
        dst: rule.dst,
        proto: rule.proto,
        sport: rule.sport,
        dport: rule.dport,
        tag: rule.tag.unwrap_or_else(|| "vectoros".to_string()),
    };

    match acl.add_rule(&acl_rule) {
        Ok(index) => Json(json!({
            "status": "ok",
            "acl_index": index,
            "message": format!("ACL rule added (index: {})", index)
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP ACL rules
#[utoipa::path(
    get,
    path = "/api/vpp/acl/list",
    tag = "VPP ACL",
    responses(
        (status = 200, description = "ACL rules list", body = Value)
    )
)]
pub async fn list_vpp_acls() -> Json<Value> {
    let acl = crate::services::vpp_acl::VppAclManager::new();
    match acl.show_rules() {
        Ok(rules) => Json(json!({ "rules": rules, "count": rules.len() })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Delete VPP ACL rule
#[utoipa::path(
    delete,
    path = "/api/vpp/acl/{index}",
    tag = "VPP ACL",
    params(
        ("index" = u32, Path, description = "ACL rule index")
    ),
    responses(
        (status = 200, description = "ACL rule deleted", body = Value)
    )
)]
pub async fn delete_vpp_acl(Path(index): Path<u32>) -> Json<Value> {
    let acl = crate::services::vpp_acl::VppAclManager::new();
    match acl.delete_rule(index) {
        Ok(()) => Json(json!({ "status": "ok", "message": format!("ACL rule {} deleted", index) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Apply VPP ACL to interface
#[utoipa::path(
    post,
    path = "/api/vpp/acl/apply",
    tag = "VPP ACL",
    request_body = VppAclApply,
    responses(
        (status = 200, description = "ACL applied", body = Value)
    )
)]
pub async fn apply_vpp_acl(
    Json(req): Json<VppAclApply>,
) -> Json<Value> {
    let acl = crate::services::vpp_acl::VppAclManager::new();
    match acl.apply_to_interface(&req.interface, req.acl_index, req.input) {
        Ok(()) => Json(json!({
            "status": "ok",
            "message": format!("ACL {} applied to {} {}", req.acl_index, req.interface, if req.input { "input" } else { "output" })
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VppAclRule {
    pub action: String,
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub proto: u8,
    #[serde(default = "default_sport")]
    pub sport: String,
    #[serde(default = "default_dport")]
    pub dport: String,
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VppAclApply {
    pub interface: String,
    pub acl_index: u32,
    #[serde(default = "default_true")]
    pub input: bool,
}

fn default_sport() -> String { "0-65535".to_string() }
fn default_dport() -> String { "0-65535".to_string() }
fn default_true() -> bool { true }

/// Generate WireGuard keypair
#[utoipa::path(
    get,
    path = "/api/vpp/wireguard/genkey",
    tag = "VPP WireGuard",
    responses(
        (status = 200, description = "Generated keypair", body = Value)
    )
)]
pub async fn generate_wireguard_keypair() -> Json<Value> {
    let wg = crate::services::vpp_wireguard::VppWireGuardManager::new();
    match wg.generate_keypair() {
        Ok((private_key, public_key)) => Json(json!({
            "private_key": private_key,
            "public_key": public_key
        })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP WireGuard interfaces
#[utoipa::path(
    get,
    path = "/api/vpp/wireguard/interfaces",
    tag = "VPP WireGuard",
    responses(
        (status = 200, description = "WireGuard interfaces", body = Value)
    )
)]
pub async fn list_wireguard_interfaces() -> Json<Value> {
    let wg = crate::services::vpp_wireguard::VppWireGuardManager::new();
    match wg.show_interfaces() {
        Ok(interfaces) => Json(json!({ "interfaces": interfaces })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP IPSec SAs
#[utoipa::path(
    get,
    path = "/api/vpp/ipsec/sa",
    tag = "VPP IPSec",
    responses(
        (status = 200, description = "IPSec Security Associations", body = Value)
    )
)]
pub async fn get_ipsec_sa() -> Json<Value> {
    let ipsec = crate::services::vpp_ipsec::VppIpSecManager::new();
    match ipsec.show_sa() {
        Ok(sas) => Json(json!({ "sas": sas, "count": sas.len() })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP IPSec profiles
#[utoipa::path(
    get,
    path = "/api/vpp/ipsec/profiles",
    tag = "VPP IPSec",
    responses(
        (status = 200, description = "IPSec profiles", body = Value)
    )
)]
pub async fn get_ipsec_profiles() -> Json<Value> {
    let ipsec = crate::services::vpp_ipsec::VppIpSecManager::new();
    match ipsec.show_profiles() {
        Ok(profiles) => Json(json!({ "profiles": profiles, "count": profiles.len() })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP GRE tunnels
#[utoipa::path(
    get,
    path = "/api/vpp/tunnels/gre",
    tag = "VPP Tunnels",
    responses(
        (status = 200, description = "GRE tunnels", body = Value)
    )
)]
pub async fn get_gre_tunnels() -> Json<Value> {
    let tunnel = crate::services::vpp_tunnel::VppTunnelManager::new();
    match tunnel.show_gre() {
        Ok(tunnels) => Json(json!({ "tunnels": tunnels, "count": tunnels.len() })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Show VPP VXLAN tunnels
#[utoipa::path(
    get,
    path = "/api/vpp/tunnels/vxlan",
    tag = "VPP Tunnels",
    responses(
        (status = 200, description = "VXLAN tunnels", body = Value)
    )
)]
pub async fn get_vxlan_tunnels() -> Json<Value> {
    let tunnel = crate::services::vpp_tunnel::VppTunnelManager::new();
    match tunnel.show_vxlan() {
        Ok(tunnels) => Json(json!({ "tunnels": tunnels, "count": tunnels.len() })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Request body for creating a GRE tunnel
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGreTunnelRequest {
    pub src: String,
    pub dst: String,
}

/// Create a GRE tunnel
#[utoipa::path(
    post,
    path = "/api/vpp/tunnels/gre/create",
    tag = "VPP Tunnels",
    request_body = CreateGreTunnelRequest,
    responses(
        (status = 200, description = "GRE tunnel created", body = Value),
        (status = 400, description = "Invalid request", body = Value)
    )
)]
pub async fn create_gre_tunnel(
    Json(req): Json<CreateGreTunnelRequest>,
) -> Json<Value> {
    if req.src.is_empty() || req.dst.is_empty() {
        return Json(json!({ "error": "Source and destination IPs are required" }));
    }
    let tunnel = crate::services::vpp_tunnel::VppTunnelManager::new();
    match tunnel.create_gre(&req.src, &req.dst) {
        Ok(iface) => Json(json!({ "status": "ok", "interface": iface, "message": format!("GRE tunnel {} created", iface) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Request body for creating a VXLAN tunnel
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateVxlanTunnelRequest {
    pub src: String,
    pub dst: String,
    pub vni: u32,
}

/// Create a VXLAN tunnel
#[utoipa::path(
    post,
    path = "/api/vpp/tunnels/vxlan/create",
    tag = "VPP Tunnels",
    request_body = CreateVxlanTunnelRequest,
    responses(
        (status = 200, description = "VXLAN tunnel created", body = Value),
        (status = 400, description = "Invalid request", body = Value)
    )
)]
pub async fn create_vxlan_tunnel(
    Json(req): Json<CreateVxlanTunnelRequest>,
) -> Json<Value> {
    if req.src.is_empty() || req.dst.is_empty() {
        return Json(json!({ "error": "Source and destination IPs are required" }));
    }
    let tunnel = crate::services::vpp_tunnel::VppTunnelManager::new();
    match tunnel.create_vxlan(&req.src, &req.dst, req.vni) {
        Ok(iface) => Json(json!({ "status": "ok", "interface": iface, "message": format!("VXLAN tunnel {} created", iface) })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

/// Request body for deleting a tunnel
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteTunnelRequest {
    pub name: String,
}

/// Delete a tunnel by interface name
#[utoipa::path(
    delete,
    path = "/api/vpp/tunnels/delete",
    tag = "VPP Tunnels",
    request_body = DeleteTunnelRequest,
    responses(
        (status = 200, description = "Tunnel deleted", body = Value),
        (status = 400, description = "Invalid request", body = Value)
    )
)]
pub async fn delete_tunnel(
    Json(req): Json<DeleteTunnelRequest>,
) -> Json<Value> {
    if req.name.is_empty() {
        return Json(json!({ "error": "Tunnel name is required" }));
    }
    let tunnel = crate::services::vpp_tunnel::VppTunnelManager::new();
    match tunnel.delete_tunnel(&req.name) {
        Ok(msg) => Json(json!({ "status": "ok", "message": msg })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}
