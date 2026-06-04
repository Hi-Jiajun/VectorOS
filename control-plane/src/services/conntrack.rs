//! Connection tracking service
//!
//! Provides connection monitoring by calling VPP's NAT session show commands
//! via the Python conntrack_manager wrapper. Reports active NAT sessions,
//! connection statistics, protocol distribution, and top talkers.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

const CONNTRACK_MANAGER_SCRIPT: &str = "/root/VectorOS/vpp-tools/conntrack_manager.py";

/// A single tracked connection (NAT session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub protocol: String,
    pub src_ip: String,
    pub src_port: u32,
    pub dst_ip: String,
    pub dst_port: u32,
    pub state: String,
    #[serde(default)]
    pub nat_src_ip: String,
    #[serde(default)]
    pub nat_src_port: u32,
    #[serde(default)]
    pub direction: String,
}

/// Protocol distribution counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDistribution {
    pub tcp: u32,
    pub udp: u32,
    pub icmp: u32,
    pub other: u32,
}

/// Connection statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConntrackStats {
    pub total_connections: u32,
    pub new_connections: u32,
    pub established_connections: u32,
    pub other_connections: u32,
    pub protocol_distribution: ProtocolDistribution,
    #[serde(default)]
    pub state_distribution: std::collections::HashMap<String, u32>,
    #[serde(default)]
    pub top_tcp_dst_ports: Vec<PortCount>,
    #[serde(default)]
    pub top_udp_dst_ports: Vec<PortCount>,
}

/// Port hit count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortCount {
    pub port: u32,
    pub count: u32,
}

/// A top talker entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTalker {
    pub address: String,
    pub connection_count: u32,
}

/// Connection tracking status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConntrackStatus {
    pub tracking_active: bool,
    pub data_source: String,
    pub stats: ConntrackStats,
    pub nat_interfaces: Vec<NatInterface>,
    #[serde(default)]
    pub nat_summary: String,
    #[serde(default)]
    pub arp_neighbor_count: u32,
}

/// NAT interface entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatInterface {
    pub name: String,
    pub direction: String,
}

/// Connection list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionListResponse {
    pub total: u32,
    pub data_source: String,
    pub connections: Vec<Connection>,
}

/// Top talkers response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTalkersResponse {
    pub total_connections: u32,
    pub top_sources: Vec<TopTalker>,
    pub top_destinations: Vec<TopTalker>,
    pub protocol_distribution: ProtocolDistribution,
}

/// Filter request for connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConntrackFilter {
    pub ip: Option<String>,
    pub port: Option<u32>,
    pub protocol: Option<String>,
}

/// Run the conntrack_manager.py script with the given action and optional args.
fn run_conntrack_manager(action: &str, extra_args: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut cmd = Command::new("python3");
    cmd.arg(CONNTRACK_MANAGER_SCRIPT);
    cmd.arg(action);

    for (key, value) in extra_args {
        cmd.arg(format!("--{}", key));
        cmd.arg(value);
    }

    let output = cmd
        .output()
        .context("Failed to execute conntrack_manager.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("conntrack_manager.py failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).with_context(|| {
        format!(
            "Failed to parse conntrack_manager.py output: {}",
            &stdout[..stdout.len().min(200)]
        )
    })
}

/// Execute a vppctl command directly for supplementary data.
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

/// Get connection tracking status overview.
pub fn get_status() -> Result<serde_json::Value> {
    run_conntrack_manager("status", &[])
}

/// List active connections (NAT sessions).
pub fn list_connections() -> Result<serde_json::Value> {
    run_conntrack_manager("connections", &[])
}

/// Get connection statistics.
pub fn get_stats() -> Result<serde_json::Value> {
    run_conntrack_manager("stats", &[])
}

/// Get top talkers (source and destination IPs).
pub fn get_top_talkers() -> Result<serde_json::Value> {
    run_conntrack_manager("top", &[])
}

/// Filter connections by IP, port, or protocol.
pub fn filter_connections(filter: &ConntrackFilter) -> Result<serde_json::Value> {
    let mut extra_args: Vec<(&str, &str)> = Vec::new();
    let ip_str;
    let port_str;
    let proto_str;

    if let Some(ref ip) = filter.ip {
        ip_str = ip.clone();
        extra_args.push(("ip", &ip_str));
    }
    if let Some(port) = filter.port {
        port_str = port.to_string();
        extra_args.push(("port", &port_str));
    }
    if let Some(ref proto) = filter.protocol {
        proto_str = proto.clone();
        extra_args.push(("protocol", &proto_str));
    }

    run_conntrack_manager("filter", &extra_args)
}

/// Get supplementary NAT data directly from VPP.
pub fn get_nat_detail() -> Result<serde_json::Value> {
    let mut result = serde_json::json!({});

    // Get NAT44 EI sessions raw
    let sessions_raw = run_vppctl(&["show", "nat44", "ei", "sessions"])
        .unwrap_or_default();
    result["sessions_raw"] = serde_json::json!(sessions_raw);

    // Get NAT44 EI addresses (summary is not available in all VPP versions)
    let summary = run_vppctl(&["show", "nat44", "ei", "addresses"])
        .unwrap_or_default();
    result["summary"] = serde_json::json!(summary);

    // Get NAT44 EI interfaces
    let interfaces = run_vppctl(&["show", "nat44", "ei", "interfaces"])
        .unwrap_or_default();
    result["interfaces"] = serde_json::json!(interfaces);

    // Get IP neighbors count
    let neighbors = run_vppctl(&["show", "ip", "neighbors"])
        .unwrap_or_default();
    let neighbor_count = neighbors
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("IP") && !l.starts_with("---"))
        .count();
    result["arp_neighbor_count"] = serde_json::json!(neighbor_count as u32);

    Ok(result)
}
