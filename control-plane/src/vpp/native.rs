use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, warn};

/// Native VPP command execution
/// Replaces Python subprocess calls with direct vppctl commands

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub sw_if_index: u32,
    pub state: String,
    pub mtu: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeStatus {
    pub status: String,
    pub clients: Vec<PppoeClient>,
    pub interfaces: Vec<InterfaceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeClient {
    pub sw_if_index: u32,
    pub pppox_sw_if_index: u32,
    pub session_id: u16,
    pub client_state: u8,
    pub auth_user: String,
    pub ipv4_local: String,
    pub ipv4_peer: String,
    pub mtu: u32,
    pub mru: u32,
    pub use_peer_dns: bool,
    pub add_default_route4: bool,
    pub add_default_route6: bool,
    pub session_uptime_seconds: u32,
    pub total_reconnects: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatStatus {
    pub enabled: bool,
    pub interfaces: Vec<NatInterface>,
    pub session_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatInterface {
    pub name: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_percent: f64,
    pub cpu_count: u32,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_percent: f64,
    pub disk_total: u64,
    pub disk_used: u64,
    pub disk_percent: f64,
    pub vpp_version: String,
    pub interface_count: u32,
}

/// Execute a vppctl command
fn run_vppctl(args: &[&str]) -> Result<String> {
    let output = Command::new("vppctl")
        .args(args)
        .output()
        .context("Failed to execute vppctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("vppctl failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get list of interfaces
pub fn get_interfaces() -> Result<Vec<InterfaceInfo>> {
    let output = run_vppctl(&["show", "interface"])?;
    let mut interfaces = Vec::new();

    for line in output.lines() {
        if line.is_empty() || line.contains("Name") || line.contains("---") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let name = parts[0].to_string();
            let sw_if_index = parts[1].parse().unwrap_or(0);
            let state = parts[2].to_string();
            let mtu = parts[3].split('/').next().unwrap_or("0").parse().unwrap_or(0);

            interfaces.push(InterfaceInfo {
                name,
                sw_if_index,
                state,
                mtu,
            });
        }
    }

    Ok(interfaces)
}

/// Set interface state (up/down)
pub fn set_interface_state(name: &str, state: &str) -> Result<()> {
    run_vppctl(&["set", "interface", "state", name, state])?;
    Ok(())
}

/// Set interface IP address
pub fn set_interface_ip(name: &str, ip: &str) -> Result<()> {
    run_vppctl(&["set", "interface", "ip", "address", name, ip])?;
    Ok(())
}

/// Get PPPoE status
pub fn get_pppoe_status() -> Result<PppoeStatus> {
    let interfaces = get_interfaces()?;

    // Get PPPoE clients
    let output = run_vppctl(&["show", "pppoe", "client"])?;
    let mut clients = Vec::new();

    for line in output.lines() {
        if line.contains("sw-if-index") {
            // Parse PPPoE client line
            // Format: [0] sw-if-index 1 host-uniq 1 pppox-sw-if-index 4 state PPPOE_CLIENT_DISCOVERY session-id 0
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 12 {
                let sw_if_index = parts[2].parse().unwrap_or(0);
                let pppox_sw_if_index = parts[6].parse().unwrap_or(0);
                let session_id = parts[10].parse().unwrap_or(0);
                let state_str = parts[8];

                let client_state = match state_str {
                    "PPPOE_CLIENT_DISCOVERY" => 0,
                    "PPPOE_CLIENT_REQUEST" => 1,
                    "PPPOE_CLIENT_SESSION" => 2,
                    _ => 0,
                };

                clients.push(PppoeClient {
                    sw_if_index,
                    pppox_sw_if_index,
                    session_id,
                    client_state,
                    auth_user: String::new(),
                    ipv4_local: "0.0.0.0".to_string(),
                    ipv4_peer: "0.0.0.0".to_string(),
                    mtu: 1492,
                    mru: 1492,
                    use_peer_dns: true,
                    add_default_route4: true,
                    add_default_route6: true,
                    session_uptime_seconds: 0,
                    total_reconnects: 0,
                });
            }
        }
    }

    Ok(PppoeStatus {
        status: "ok".to_string(),
        clients,
        interfaces,
    })
}

/// Get NAT status
pub fn get_nat_status() -> Result<NatStatus> {
    let output = run_vppctl(&["show", "nat44", "ei", "interfaces"])?;
    let mut interfaces = Vec::new();
    let mut enabled = false;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains("NAT44 interfaces:") {
            continue;
        }
        if line.contains(" in") || line.contains(" out") {
            enabled = true;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                interfaces.push(NatInterface {
                    name: parts[0].to_string(),
                    direction: parts[1].to_string(),
                });
            }
        }
    }

    Ok(NatStatus {
        enabled,
        interfaces,
        session_count: 0,
    })
}

/// Get system information
pub fn get_system_info() -> Result<SystemInfo> {
    // Get VPP version
    let vpp_version = run_vppctl(&["show", "version"]).unwrap_or_default();

    // Get interface count
    let interfaces = get_interfaces().unwrap_or_default();
    let interface_count = interfaces.len() as u32;

    // Get system stats from /proc
    let cpu_percent = get_cpu_usage()?;
    let (memory_total, memory_used, memory_percent) = get_memory_usage()?;
    let (disk_total, disk_used, disk_percent) = get_disk_usage()?;
    let cpu_count = num_cpus::get() as u32;

    Ok(SystemInfo {
        cpu_percent,
        cpu_count,
        memory_total,
        memory_used,
        memory_percent,
        disk_total,
        disk_used,
        disk_percent,
        vpp_version,
        interface_count,
    })
}

fn get_cpu_usage() -> Result<f64> {
    let output = Command::new("grep")
        .args(&["cpu ", "/proc/stat"])
        .output()?;
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 5 {
        let user: u64 = parts[1].parse().unwrap_or(0);
        let nice: u64 = parts[2].parse().unwrap_or(0);
        let system: u64 = parts[3].parse().unwrap_or(0);
        let idle: u64 = parts[4].parse().unwrap_or(0);
        let total = user + nice + system + idle;
        if total > 0 {
            return Ok(((total - idle) as f64 / total as f64) * 100.0);
        }
    }
    Ok(0.0)
}

fn get_memory_usage() -> Result<(u64, u64, f64)> {
    let output = Command::new("free").args(&["-b"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("Mem:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let total: u64 = parts[1].parse().unwrap_or(0);
                let used: u64 = parts[2].parse().unwrap_or(0);
                let percent = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };
                return Ok((total, used, percent));
            }
        }
    }
    Ok((0, 0, 0.0))
}

fn get_disk_usage() -> Result<(u64, u64, f64)> {
    let output = Command::new("df").args(&["-B1", "/"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let total: u64 = parts[1].parse().unwrap_or(0);
            let used: u64 = parts[2].parse().unwrap_or(0);
            let percent = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };
            return Ok((total, used, percent));
        }
    }
    Ok((0, 0, 0.0))
}
