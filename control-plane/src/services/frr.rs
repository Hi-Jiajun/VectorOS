//! FRRouting (FRR) integration service
//!
//! Manages FRRouting via vtysh and the FPM (Forwarding Plane Manager) socket.
//! Supports BGP and OSPF configuration and route management.

use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// FRRouting service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrrConfig {
    /// BGP AS number (None if BGP is disabled)
    pub bgp_as: Option<u32>,
    /// OSPF process ID (None if OSPF is disabled)
    pub ospf_process_id: Option<u32>,
    /// Static routes to add
    pub static_routes: Vec<StaticRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticRoute {
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub distance: Option<u32>,
}

/// Status of FRRouting daemons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrrStatus {
    pub running: bool,
    pub version: String,
    pub daemons: std::collections::HashMap<String, bool>,
}

/// A route entry from the routing table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEntry {
    pub protocol: String,
    pub prefix: String,
    pub nexthop: Option<String>,
    pub interface: Option<String>,
    pub raw: String,
}

/// FPM message types (Forwarding Plane Manager protocol)
#[allow(dead_code)]
const FPM_MSG_ADD: u8 = 1;
#[allow(dead_code)]
const FPM_MSG_DELETE: u8 = 2;

impl Default for FrrConfig {
    fn default() -> Self {
        Self {
            bgp_as: None,
            ospf_process_id: Some(1),
            static_routes: Vec::new(),
        }
    }
}

/// Run a vtysh command and return the output
fn run_vtysh(cmd: &str) -> Result<String, String> {
    let output = Command::new("vtysh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| format!("Failed to run vtysh: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("vtysh error: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if FRRouting is running
pub fn is_running() -> bool {
    match run_vtysh("show version") {
        Ok(output) => !output.is_empty(),
        Err(_) => false,
    }
}

/// Get FRRouting status
pub fn get_status() -> Result<FrrStatus, String> {
    let running = is_running();
    let mut version = String::new();
    let mut daemons = std::collections::HashMap::new();

    if running {
        // Get version
        if let Ok(output) = run_vtysh("show version") {
            for line in output.lines() {
                if line.contains("FRRouting") || line.to_lowercase().contains("frr") {
                    version = line.trim().to_string();
                    break;
                }
            }
            if version.is_empty() {
                version = output.lines().next().unwrap_or("").trim().to_string();
            }
        }

        // Get daemon status
        if let Ok(output) = run_vtysh("show daemons") {
            for line in output.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with("Daemon") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(name) = parts.first() {
                        let is_active = line.to_lowercase().contains("running")
                            || line.contains("*");
                        daemons.insert(name.to_string(), is_active);
                    }
                }
            }
        }
    }

    Ok(FrrStatus {
        running,
        version,
        daemons,
    })
}

/// Show routes from FRRouting
pub fn show_routes() -> Result<Vec<RouteEntry>, String> {
    let output = run_vtysh("show ip route")?;
    let mut routes = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Codes") || line.starts_with("Gateway") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let protocol = match parts[0] {
                "C" => "connected",
                "S" => "static",
                "O" | "OI" | "OE1" | "OE2" | "ON1" | "ON2" => "ospf",
                "B" | "BI" | "B*>" => "bgp",
                "K" => "kernel",
                _ => "other",
            };

            routes.push(RouteEntry {
                protocol: protocol.to_string(),
                prefix: parts.get(1).unwrap_or(&"").to_string(),
                nexthop: None,
                interface: None,
                raw: line.to_string(),
            });
        }
    }

    Ok(routes)
}

/// Add a static route
pub fn add_route(prefix: &str, nexthop: Option<&str>, interface: Option<&str>,
                 distance: Option<u32>) -> Result<String, String> {
    let mut cmd = format!("conf\nip route");
    if let Some(d) = distance {
        cmd.push_str(&format!(" {}", d));
    }
    cmd.push_str(&format!(" {}", prefix));
    if let Some(nh) = nexthop {
        cmd.push_str(&format!(" {}", nh));
    }
    if let Some(iface) = interface {
        cmd.push_str(&format!(" {}", iface));
    }
    cmd.push_str("\nexit");

    run_vtysh(&cmd)?;
    info!("Static route {} added via FRR", prefix);
    Ok(format!("Route {} added", prefix))
}

/// Delete a static route
pub fn del_route(prefix: &str, nexthop: Option<&str>, interface: Option<&str>,
                 distance: Option<u32>) -> Result<String, String> {
    let mut cmd = format!("conf\nno ip route");
    if let Some(d) = distance {
        cmd.push_str(&format!(" {}", d));
    }
    cmd.push_str(&format!(" {}", prefix));
    if let Some(nh) = nexthop {
        cmd.push_str(&format!(" {}", nh));
    }
    if let Some(iface) = interface {
        cmd.push_str(&format!(" {}", iface));
    }
    cmd.push_str("\nexit");

    run_vtysh(&cmd)?;
    info!("Static route {} deleted via FRR", prefix);
    Ok(format!("Route {} deleted", prefix))
}

/// Configure BGP
pub fn configure_bgp(as_number: u32, neighbor_ip: Option<&str>,
                     neighbor_as: Option<u32>, networks: &[String]) -> Result<String, String> {
    let mut cmd = format!("conf\nrouter bgp {}\n", as_number);

    if let Some(nh_ip) = neighbor_ip {
        if let Some(nh_as) = neighbor_as {
            cmd.push_str(&format!("neighbor {} remote-as {}\n", nh_ip, nh_as));
        }
    }

    for network in networks {
        cmd.push_str(&format!("network {}\n", network));
    }

    cmd.push_str("exit");
    run_vtysh(&cmd)?;

    info!("BGP AS {} configured", as_number);
    Ok(format!("BGP AS {} configured", as_number))
}

/// Configure OSPF
pub fn configure_ospf(process_id: u32, networks: &[(String, String)],
                      redistribute_connected: bool) -> Result<String, String> {
    let mut cmd = format!("conf\nrouter ospf {}\n", process_id);

    for (prefix, area) in networks {
        cmd.push_str(&format!("network {} area {}\n", prefix, area));
    }

    if redistribute_connected {
        cmd.push_str("redistribute connected\n");
    }

    cmd.push_str("exit");
    run_vtysh(&cmd)?;

    info!("OSPF process {} configured", process_id);
    Ok(format!("OSPF process {} configured", process_id))
}

/// Connect to the FPM socket and send route updates
/// FPM protocol: 4-byte length (including this field) + 1-byte message type + route data
#[allow(dead_code)]
pub fn connect_fpm(socket_path: &str) -> Result<std::os::unix::net::UnixStream, String> {
    use std::os::unix::net::UnixStream;

    let stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("Failed to connect to FPM socket {}: {}", socket_path, e))?;

    info!("Connected to FPM socket at {}", socket_path);
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frr_config_default() {
        let config = FrrConfig::default();
        assert!(config.bgp_as.is_none());
        assert_eq!(config.ospf_process_id, Some(1));
        assert!(config.static_routes.is_empty());
    }
}
