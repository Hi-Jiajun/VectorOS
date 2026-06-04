use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
/// Path to the VPP performance stats Python script
const VPP_STATS_SCRIPT: &str = "/home/hiliang/Github/vectoros/vpp-tools/vpp_stats.py";

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
pub struct InterfaceStats {
    pub name: String,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_drops: u64,
    pub tx_drops: u64,
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

// ── VPP Performance Metrics ─────────────────────────────────────────

/// Packet processing rate (packets and bytes per second)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketRate {
    pub rx_packets_per_sec: f64,
    pub tx_packets_per_sec: f64,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

/// Per-interface throughput detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceThroughput {
    pub name: String,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_pps: f64,
    pub tx_pps: f64,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

/// NAT session statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatPerformance {
    pub session_count: u32,
    pub session_rate: f64,
}

/// PPPoE session statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoePerformance {
    pub total_clients: u32,
    pub sessions_active: u32,
    pub sessions_discovery: u32,
}

/// VPP heap memory usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppMemory {
    pub used: u64,
    pub free: u64,
    pub total: u64,
    pub percent: f64,
}

/// Worker thread info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppThreads {
    pub worker_threads: u32,
    pub thread_details: Vec<ThreadDetail>,
}

/// Single worker thread detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub name: String,
    pub lcore: String,
}

/// Drop/error counter summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppErrors {
    pub total_drops: u64,
    pub total_errors: u64,
    pub counters: Vec<ErrorCounter>,
}

/// Single error counter entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCounter {
    pub name: String,
    pub count: u64,
}

/// Complete VPP performance snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppPerformance {
    pub timestamp: f64,
    pub packet_rate: PacketRate,
    pub interfaces: Vec<InterfaceThroughput>,
    pub nat: NatPerformance,
    pub pppoe: PppoePerformance,
    pub memory: VppMemory,
    pub threads: VppThreads,
    pub errors: VppErrors,
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

/// Remove IP address from interface
pub fn remove_interface_ip(name: &str, ip: &str) -> Result<()> {
    run_vppctl(&["set", "interface", "ip", "address", name, ip, "del"])?;
    Ok(())
}

/// Set interface MTU (packet size)
pub fn set_interface_mtu(name: &str, mtu: u32) -> Result<()> {
    let mtu_str = mtu.to_string();
    run_vppctl(&["set", "interface", "mtu", "packet", &mtu_str, name])?;
    Ok(())
}

/// Enable promiscuous mode on interface
pub fn enable_interface_promisc(name: &str) -> Result<()> {
    run_vppctl(&["set", "interface", "promiscuous", "on", name])?;
    Ok(())
}

/// Disable promiscuous mode on interface
pub fn disable_interface_promisc(name: &str) -> Result<()> {
    run_vppctl(&["set", "interface", "promiscuous", "off", name])?;
    Ok(())
}

/// Get detailed interface statistics (RX/TX packets, bytes, errors, drops)
pub fn get_interface_stats(name: &str) -> Result<InterfaceStats> {
    let mut stats = InterfaceStats {
        name: name.to_string(),
        rx_packets: 0,
        tx_packets: 0,
        rx_bytes: 0,
        tx_bytes: 0,
        rx_errors: 0,
        tx_errors: 0,
        rx_drops: 0,
        tx_drops: 0,
    };

    // Use "show interface" to get detailed stats
    let output = run_vppctl(&["show", "interface", name])?;
    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if let Some(idx) = lower.find("rx packets") {
            let rest = &lower[idx..];
            let num: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.rx_packets = num.parse().unwrap_or(0);
        } else if let Some(idx) = lower.find("tx packets") {
            let rest = &lower[idx..];
            let num: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.tx_packets = num.parse().unwrap_or(0);
        } else if let Some(idx) = lower.find("rx bytes") {
            let rest = &lower[idx..];
            let num: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.rx_bytes = num.parse().unwrap_or(0);
        } else if let Some(idx) = lower.find("tx bytes") {
            let rest = &lower[idx..];
            let num: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.tx_bytes = num.parse().unwrap_or(0);
        } else if lower.contains("rx error") || lower.contains("rx err") {
            let num: String = lower.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.rx_errors = num.parse().unwrap_or(0);
        } else if lower.contains("tx error") || lower.contains("tx err") {
            let num: String = lower.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.tx_errors = num.parse().unwrap_or(0);
        } else if lower.contains("rx drop") {
            let num: String = lower.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.rx_drops = num.parse().unwrap_or(0);
        } else if lower.contains("tx drop") {
            let num: String = lower.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| c.is_ascii_digit()).collect();
            stats.tx_drops = num.parse().unwrap_or(0);
        }
    }

    Ok(stats)
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

/// Collect VPP performance metrics by calling the Python stats collector.
///
/// The Python script runs vppctl commands, calculates rates between calls
/// (using a temp file for previous values), and outputs structured JSON.
pub fn get_vpp_performance() -> Result<VppPerformance> {
    let output = Command::new("python3")
        .arg(VPP_STATS_SCRIPT)
        .output()
        .context("Failed to execute vpp_stats.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("vpp_stats.py failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: serde_json::Value = serde_json::from_str(&stdout)
        .context("Failed to parse vpp_stats.py JSON output")?;

    // Parse the JSON into our typed structs
    let perf = VppPerformance {
        timestamp: raw["timestamp"].as_f64().unwrap_or(0.0),
        packet_rate: PacketRate {
            rx_packets_per_sec: raw["packet_rate"]["rx_packets_per_sec"].as_f64().unwrap_or(0.0),
            tx_packets_per_sec: raw["packet_rate"]["tx_packets_per_sec"].as_f64().unwrap_or(0.0),
            rx_bytes_per_sec: raw["packet_rate"]["rx_bytes_per_sec"].as_f64().unwrap_or(0.0),
            tx_bytes_per_sec: raw["packet_rate"]["tx_bytes_per_sec"].as_f64().unwrap_or(0.0),
        },
        interfaces: parse_interface_throughput(&raw["interfaces"]),
        nat: NatPerformance {
            session_count: raw["nat"]["session_count"].as_u64().unwrap_or(0) as u32,
            session_rate: raw["nat"]["session_rate"].as_f64().unwrap_or(0.0),
        },
        pppoe: PppoePerformance {
            total_clients: raw["pppoe"]["total_clients"].as_u64().unwrap_or(0) as u32,
            sessions_active: raw["pppoe"]["sessions_active"].as_u64().unwrap_or(0) as u32,
            sessions_discovery: raw["pppoe"]["sessions_discovery"].as_u64().unwrap_or(0) as u32,
        },
        memory: VppMemory {
            used: raw["memory"]["used"].as_u64().unwrap_or(0),
            free: raw["memory"]["free"].as_u64().unwrap_or(0),
            total: raw["memory"]["total"].as_u64().unwrap_or(0),
            percent: raw["memory"]["percent"].as_f64().unwrap_or(0.0),
        },
        threads: VppThreads {
            worker_threads: raw["threads"]["worker_threads"].as_u64().unwrap_or(0) as u32,
            thread_details: parse_thread_details(&raw["threads"]["thread_details"]),
        },
        errors: VppErrors {
            total_drops: raw["errors"]["total_drops"].as_u64().unwrap_or(0),
            total_errors: raw["errors"]["total_errors"].as_u64().unwrap_or(0),
            counters: parse_error_counters(&raw["errors"]["counters"]),
        },
    };

    Ok(perf)
}

fn parse_interface_throughput(val: &serde_json::Value) -> Vec<InterfaceThroughput> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(InterfaceThroughput {
                name: item["name"].as_str().unwrap_or("").to_string(),
                rx_packets: item["rx_packets"].as_u64().unwrap_or(0),
                tx_packets: item["tx_packets"].as_u64().unwrap_or(0),
                rx_bytes: item["rx_bytes"].as_u64().unwrap_or(0),
                tx_bytes: item["tx_bytes"].as_u64().unwrap_or(0),
                rx_pps: item["rx_pps"].as_f64().unwrap_or(0.0),
                tx_pps: item["tx_pps"].as_f64().unwrap_or(0.0),
                rx_bps: item["rx_bps"].as_f64().unwrap_or(0.0),
                tx_bps: item["tx_bps"].as_f64().unwrap_or(0.0),
            });
        }
    }
    result
}

fn parse_thread_details(val: &serde_json::Value) -> Vec<ThreadDetail> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(ThreadDetail {
                name: item["name"].as_str().unwrap_or("").to_string(),
                lcore: item["lcore"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    result
}

fn parse_error_counters(val: &serde_json::Value) -> Vec<ErrorCounter> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(ErrorCounter {
                name: item["name"].as_str().unwrap_or("").to_string(),
                count: item["count"].as_u64().unwrap_or(0),
            });
        }
    }
    result
}
