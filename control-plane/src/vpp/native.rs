use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
/// Path to the VPP performance stats Python script
const VPP_STATS_SCRIPT: &str = "/root/VectorOS/vpp-tools/vpp_stats.py";

/// Native VPP command execution
/// Replaces Python subprocess calls with direct vppctl commands

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub sw_if_index: u32,
    pub state: String,
    pub mtu: u32,
    #[serde(default)]
    pub mac_address: String,
    #[serde(default)]
    pub ip_addresses: Vec<String>,
    #[serde(default)]
    pub interface_type: String,
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

// ── Interface Binding ────────────────────────────────────────────

/// Path to the interface bind Python script
const INTERFACE_BIND_SCRIPT: &str = "/root/VectorOS/vpp-tools/interface_bind.py";

/// A VF interface bound (or bindable) to VPP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundInterface {
    /// VPP interface name (e.g. "wan0")
    pub vpp_name: String,
    /// Linux VF interface name (e.g. "enp1s0")
    pub vf_name: String,
    /// Binding method: "rdma" or "dpdk"
    pub method: String,
    /// PCI address (if known)
    pub pci: String,
    /// Whether currently bound in VPP
    pub bound: bool,
    /// VPP sw_if_index (0 if not bound)
    pub sw_if_index: u32,
    /// Current state: "up", "down", or ""
    pub state: String,
    /// MTU value
    pub mtu: u32,
}

/// Result of a bind/unbind operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindResult {
    pub status: String,
    pub message: String,
    pub vpp_name: String,
    pub vf_name: String,
    pub method: Option<String>,
    pub pci: Option<String>,
}

/// List of all bound and available VF interfaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundInterfaceList {
    pub interfaces: Vec<BoundInterface>,
    pub available_vfs: Vec<AvailableVf>,
    pub count: u32,
}

/// A VF interface available for binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableVf {
    pub vf_name: String,
    pub pci: String,
    pub driver: String,
    pub bound: bool,
    pub suggested_vpp_name: String,
}

/// Run the interface_bind.py helper and parse JSON output
fn run_bind_script(args: &[&str]) -> Result<serde_json::Value> {
    let mut cmd_args: Vec<&str> = vec![INTERFACE_BIND_SCRIPT];
    cmd_args.extend_from_slice(args);

    let output = Command::new("python3")
        .args(&cmd_args)
        .output()
        .context("Failed to execute interface_bind.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("interface_bind.py failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).context("Failed to parse interface_bind.py output")
}

/// Bind a VF interface to VPP.
///
/// `method` can be "rdma" (default, no driver change) or "dpdk" (requires PCI address and vfio-pci).
/// If `pci` is not provided, it is auto-detected from sysfs.
pub fn bind_interface(vf_name: &str, vpp_name: &str, method: &str, pci: Option<&str>) -> Result<BindResult> {
    let mut args: Vec<&str> = vec![
        "bind",
        "--vf", vf_name,
        "--vpp-name", vpp_name,
        "--method", method,
    ];
    if let Some(p) = pci {
        args.extend_from_slice(&["--pci", p]);
    }

    let val = run_bind_script(&args)?;

    Ok(BindResult {
        status: val["status"].as_str().unwrap_or("error").to_string(),
        message: val["message"].as_str().unwrap_or("").to_string(),
        vpp_name: val["vpp_name"].as_str().unwrap_or(vpp_name).to_string(),
        vf_name: val["vf_name"].as_str().unwrap_or(vf_name).to_string(),
        method: val["method"].as_str().map(String::from),
        pci: val["pci"].as_str().map(String::from),
    })
}

/// Unbind an interface from VPP.
pub fn unbind_interface(vpp_name: &str) -> Result<BindResult> {
    let val = run_bind_script(&["unbind", "--vpp-name", vpp_name])?;

    Ok(BindResult {
        status: val["status"].as_str().unwrap_or("error").to_string(),
        message: val["message"].as_str().unwrap_or("").to_string(),
        vpp_name: val["vpp_name"].as_str().unwrap_or(vpp_name).to_string(),
        vf_name: val["vf_name"].as_str().unwrap_or("").to_string(),
        method: val["method"].as_str().map(String::from),
        pci: val["pci"].as_str().map(String::from),
    })
}

/// List all VF interfaces currently bound to VPP, plus available unbound VFs.
pub fn list_bound_interfaces() -> Result<BoundInterfaceList> {
    let val = run_bind_script(&["list"])?;

    let mut interfaces = Vec::new();
    if let Some(arr) = val["interfaces"].as_array() {
        for item in arr {
            interfaces.push(BoundInterface {
                vpp_name: item["vpp_name"].as_str().unwrap_or("").to_string(),
                vf_name: item["vf_name"].as_str().unwrap_or("").to_string(),
                method: item["method"].as_str().unwrap_or("").to_string(),
                pci: item["pci"].as_str().unwrap_or("").to_string(),
                bound: item["bound"].as_bool().unwrap_or(false),
                sw_if_index: item["sw_if_index"].as_u64().unwrap_or(0) as u32,
                state: item["state"].as_str().unwrap_or("").to_string(),
                mtu: item["mtu"].as_u64().unwrap_or(0) as u32,
            });
        }
    }

    let mut available_vfs = Vec::new();
    if let Some(arr) = val["available_vfs"].as_array() {
        for item in arr {
            available_vfs.push(AvailableVf {
                vf_name: item["vf_name"].as_str().unwrap_or("").to_string(),
                pci: item["pci"].as_str().unwrap_or("").to_string(),
                driver: item["driver"].as_str().unwrap_or("").to_string(),
                bound: item["bound"].as_bool().unwrap_or(false),
                suggested_vpp_name: item["suggested_vpp_name"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    Ok(BoundInterfaceList {
        interfaces,
        available_vfs,
        count: val["count"].as_u64().unwrap_or(0) as u32,
    })
}

/// Configure an already-bound VPP interface (IP, MTU, bring up).
pub fn configure_bound_interface(vpp_name: &str, ip: Option<&str>, mtu: Option<u32>) -> Result<serde_json::Value> {
    let mut script_args: Vec<String> = vec![
        "configure".to_string(),
        "--vpp-name".to_string(),
        vpp_name.to_string(),
    ];

    if let Some(i) = ip {
        script_args.extend_from_slice(&["--ip".to_string(), i.to_string()]);
    }

    if let Some(m) = mtu {
        script_args.extend_from_slice(&["--mtu".to_string(), m.to_string()]);
    }

    let str_refs: Vec<&str> = script_args.iter().map(|s| s.as_str()).collect();
    run_bind_script(&str_refs)
}

/// Classify interface type based on name
fn classify_interface_type(name: &str) -> String {
    if name.starts_with("local") {
        "local".to_string()
    } else if name.starts_with("host") || name.starts_with("veth") {
        "host".to_string()
    } else if name.starts_with("GigabitEthernet") || name.starts_with("TenGigabitEthernet")
        || name.starts_with("FortyGigabitEthernet") || name.starts_with("HundredGigabitEthernet")
        || name.starts_with("eth") || name.starts_with("enp")
    {
        "physical".to_string()
    } else if name.starts_with("loop") || name.starts_with("lo") {
        "loopback".to_string()
    } else if name.contains("pppoe") || name.contains("ppp") {
        "pppoe".to_string()
    } else if name.starts_with("bond") {
        "bond".to_string()
    } else if name.starts_with("tap") {
        "tap".to_string()
    } else if name.starts_with("vmxnet") || name.starts_with("virt") {
        "virtual".to_string()
    } else if name.starts_with("lan") {
        "bridge".to_string()
    } else {
        "other".to_string()
    }
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

            // Parse optional IP address (VPP output may include it after MTU)
            let mut ip_addresses = Vec::new();
            let mut mac_address = String::new();
            for part in &parts[4..] {
                if part.contains('/') && (part.contains('.') || part.contains(':')) {
                    // Looks like an IP/CIDR
                    ip_addresses.push(part.to_string());
                } else if part.contains(':') && part.len() == 17 {
                    // Looks like a MAC address (xx:xx:xx:xx:xx:xx)
                    mac_address = part.to_string();
                }
            }

            let interface_type = classify_interface_type(&name);

            interfaces.push(InterfaceInfo {
                name,
                sw_if_index,
                state,
                mtu,
                mac_address,
                ip_addresses,
                interface_type,
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
            session_count: raw["nat"]["sessions"].as_u64().unwrap_or(0) as u32,
            session_rate: raw["nat"]["session_rate"].as_f64().unwrap_or(0.0),
        },
        pppoe: PppoePerformance {
            total_clients: raw["pppoe"]["total"].as_u64().unwrap_or(0) as u32,
            sessions_active: raw["pppoe"]["active"].as_u64().unwrap_or(0) as u32,
            sessions_discovery: raw["pppoe"]["discovery"].as_u64().unwrap_or(0) as u32,
        },
        memory: VppMemory {
            used: raw["memory"]["used_mb"].as_f64().unwrap_or(0.0) as u64,
            free: raw["memory"]["free_mb"].as_f64().unwrap_or(0.0) as u64,
            total: raw["memory"]["total_mb"].as_f64().unwrap_or(0.0) as u64,
            percent: raw["memory"]["percent"].as_f64().unwrap_or(0.0),
        },
        threads: VppThreads {
            worker_threads: raw["threads"]["count"].as_u64().unwrap_or(0) as u32,
            thread_details: parse_thread_details(&raw["threads"]["threads"]),
        },
        errors: VppErrors {
            total_drops: raw["errors"]["total"].as_u64().unwrap_or(0),
            total_errors: 0,
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
