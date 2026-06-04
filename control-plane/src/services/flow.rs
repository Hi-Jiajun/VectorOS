//! Flow monitoring and NetFlow/IPFIX export service
//!
//! Collects flow statistics from VPP (active flows, bytes/packets, duration),
//! computes top talkers and protocol distributions, and manages VPP's
//! flow export plugin for NetFlow/IPFIX output.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

const FLOW_MONITOR_SCRIPT: &str = "/home/hiliang/Github/vectoros/vpp-tools/flow_monitor.py";

/// A single network flow (5-tuple + counters).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEntry {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u32,
    pub dst_port: u32,
    pub protocol: String,
    pub packets: u64,
    pub bytes: u64,
    pub duration_sec: u64,
}

/// An aggregated "top talker" record keyed by IP address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTalker {
    pub address: String,
    pub bytes: u64,
    pub packets: u64,
    pub flow_count: u32,
}

/// Protocol distribution entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDistribution {
    pub protocol: String,
    pub bytes: u64,
    pub packets: u64,
    pub flow_count: u32,
    pub percentage: f64,
}

/// Complete flow monitoring status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStatus {
    pub monitoring_active: bool,
    pub active_flows: u32,
    pub flow_source: String,
    pub export_enabled: bool,
    pub collector_ip: String,
    pub collector_port: u32,
    pub flow_plugins_found: Vec<String>,
    pub top_sources: Vec<TopTalker>,
    pub top_destinations: Vec<TopTalker>,
    pub protocol_distribution: Vec<ProtocolDistribution>,
}

/// Top talkers response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTalkersResponse {
    pub active_flows: u32,
    pub flow_source: String,
    pub top_sources_by_bytes: Vec<TopTalker>,
    pub top_destinations_by_bytes: Vec<TopTalker>,
    pub top_sources_by_packets: Vec<TopTalker>,
    pub protocol_distribution: Vec<ProtocolDistribution>,
}

/// Flow export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowExportConfig {
    pub export_enabled: bool,
    pub collector_ip: String,
    pub collector_port: u32,
    pub vpp_export_output: String,
}

/// Request to configure flow export collector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowExportSetRequest {
    pub collector_ip: String,
    pub collector_port: u32,
}

/// Flow list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowListResponse {
    pub active_flows: u32,
    pub flow_source: String,
    pub flows: Vec<FlowEntry>,
}

/// Run the flow_monitor.py script with the given action and optional args.
fn run_flow_monitor(action: &str, extra_args: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut cmd = Command::new("python3");
    cmd.arg(FLOW_MONITOR_SCRIPT);
    cmd.arg(action);

    for (key, value) in extra_args {
        cmd.arg(format!("--{}", key));
        cmd.arg(value);
    }

    let output = cmd
        .output()
        .context("Failed to execute flow_monitor.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("flow_monitor.py failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse flow_monitor.py output: {}", &stdout[..stdout.len().min(200)]))
}

/// Execute a vppctl command.
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

/// Get flow monitoring status.
pub fn get_status() -> Result<serde_json::Value> {
    run_flow_monitor("status", &[])
}

/// Get top talkers.
pub fn get_top_talkers() -> Result<serde_json::Value> {
    run_flow_monitor("top", &[])
}

/// Get current flow export configuration.
pub fn get_export_config() -> Result<serde_json::Value> {
    run_flow_monitor("export-config", &[])
}

/// Set flow export collector.
pub fn set_export_collector(collector_ip: &str, collector_port: u32) -> Result<serde_json::Value> {
    let port_str = collector_port.to_string();
    run_flow_monitor("export-set", &[
        ("collector-ip", collector_ip),
        ("collector-port", &port_str),
    ])
}

/// Enable flow export.
pub fn enable_export() -> Result<serde_json::Value> {
    run_flow_monitor("export-enable", &[])
}

/// Disable flow export.
pub fn disable_export() -> Result<serde_json::Value> {
    run_flow_monitor("export-disable", &[])
}

/// Set up classify-based flow table.
pub fn setup_classify() -> Result<serde_json::Value> {
    run_flow_monitor("classify-setup", &[])
}

/// List active flows.
pub fn list_flows() -> Result<serde_json::Value> {
    run_flow_monitor("flows", &[])
}
