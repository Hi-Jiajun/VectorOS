//! Traffic control service
//!
//! Manages VPP-based traffic shaping, per-IP limits, priority queues,
//! application QoS classes, and burst control. Configuration is persisted
//! to a JSON file and applied via vppctl commands.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

const TRAFFIC_FILE: &str = "/etc/vectoros/traffic-control.json";

// ── Data structures ────────────────────────────────────────────────

/// Top-level traffic control configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrafficConfig {
    #[serde(default)]
    pub interface_limits: HashMap<String, InterfaceLimit>,
    #[serde(default)]
    pub ip_limits: HashMap<String, IpLimit>,
    #[serde(default)]
    pub priority_queues: HashMap<String, PriorityQueue>,
    #[serde(default)]
    pub app_classes: HashMap<String, AppClass>,
    #[serde(default = "default_global_enabled")]
    pub global_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_control: Option<BurstControl>,
}

fn default_global_enabled() -> bool {
    true
}

/// Per-interface bandwidth limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceLimit {
    pub rate: u64,
    pub burst: u64,
    pub direction: String,
    pub policer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

/// Per-IP bandwidth limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpLimit {
    pub rate: u64,
    pub burst: u64,
    pub policer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

/// Priority queue definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueue {
    pub level: String,
    pub weight: u32,
    pub dscp: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Application-based QoS class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppClass {
    pub ports: String,
    pub protocol: String,
    pub priority: String,
    pub dscp: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Burst control settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstControl {
    pub enabled: bool,
    pub default_burst_bytes: u64,
    pub max_burst_multiplier: f64,
}

// ── Request types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetInterfaceLimitRequest {
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_direction")]
    pub direction: String,
}

fn default_direction() -> String {
    "both".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetIpLimitRequest {
    pub ip: String,
    pub rate: u64,
    pub burst: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPriorityRequest {
    pub name: String,
    pub queue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAppClassRequest {
    pub name: String,
    #[serde(default)]
    pub ports: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dscp: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_priority() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBurstControlRequest {
    pub enabled: bool,
    #[serde(default = "default_burst_bytes")]
    pub burst_bytes: u64,
    #[serde(default = "default_multiplier")]
    pub max_burst_multiplier: f64,
}

fn default_burst_bytes() -> u64 {
    150_000
}
fn default_multiplier() -> f64 {
    1.5
}

// ── VPP helpers ────────────────────────────────────────────────────

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

/// Format bits/sec to human readable
fn format_rate(bits: u64) -> String {
    if bits >= 1_000_000_000 {
        format!("{:.1} Gbps", bits as f64 / 1_000_000_000.0)
    } else if bits >= 1_000_000 {
        format!("{:.1} Mbps", bits as f64 / 1_000_000.0)
    } else if bits >= 1_000 {
        format!("{:.1} Kbps", bits as f64 / 1_000.0)
    } else {
        format!("{} bps", bits)
    }
}

// ── Config persistence ─────────────────────────────────────────────

fn load_config() -> TrafficConfig {
    let path = Path::new(TRAFFIC_FILE);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<TrafficConfig>(&content) {
                return data;
            }
        }
    }
    TrafficConfig::default()
}

fn save_config(config: &TrafficConfig) -> Result<()> {
    let path = Path::new(TRAFFIC_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create traffic control config directory")?;
    }
    let json = serde_json::to_string_pretty(config).context("Failed to serialize traffic config")?;
    fs::write(path, json).context("Failed to write traffic control config file")?;
    Ok(())
}

// ── Public API ─────────────────────────────────────────────────────

/// Get full traffic control status.
pub fn show_status() -> Result<serde_json::Value> {
    let config = load_config();

    let vpp_policers = run_vppctl(&["show", "policer"]).unwrap_or_else(|_| "N/A".to_string());
    let vpp_policers_verbose =
        run_vppctl(&["show", "policer", "verbose"]).unwrap_or_else(|_| "N/A".to_string());
    let vpp_classify = run_vppctl(&["show", "classify", "table"]).unwrap_or_else(|_| "N/A".to_string());

    Ok(serde_json::json!({
        "status": "ok",
        "interface_limits": config.interface_limits,
        "ip_limits": config.ip_limits,
        "priority_queues": config.priority_queues,
        "app_classes": config.app_classes,
        "burst_control": config.burst_control,
        "global_enabled": config.global_enabled,
        "vpp_policers": vpp_policers,
        "vpp_policers_verbose": vpp_policers_verbose,
        "vpp_classify": vpp_classify,
        "total_interface_limits": config.interface_limits.len(),
        "total_ip_limits": config.ip_limits.len(),
        "total_app_classes": config.app_classes.len(),
    }))
}

/// Set per-interface bandwidth limit via VPP policer.
pub fn set_interface_limit(iface: &str, req: SetInterfaceLimitRequest) -> Result<serde_json::Value> {
    let mut config = load_config();
    let policer_name = format!("tc-{}-limit", iface);

    // Clean up existing
    let _ = run_vppctl(&["set", "interface", "input", "policer", iface, "0"]);
    let _ = run_vppctl(&["set", "interface", "output", "policer", iface, "0"]);
    let _ = run_vppctl(&["policer", "del", &policer_name]);

    // Create policer (rate in kbps)
    let rate_kbps = req.rate / 1000;
    let create_args = [
        "policer",
        "add",
        &policer_name,
        &rate_kbps.to_string(),
        &req.burst.to_string(),
        "type",
        "single_rate_two_color",
    ];
    run_vppctl(&create_args)?;

    // Apply
    let apply_args: Vec<&str> = match req.direction.as_str() {
        "input" => vec!["set", "interface", "input", "policer", iface, &policer_name],
        "output" => vec!["set", "interface", "output", "policer", iface, &policer_name],
        _ => vec!["set", "interface", "policer", iface, &policer_name],
    };
    run_vppctl(&apply_args)?;

    config.interface_limits.insert(
        iface.to_string(),
        InterfaceLimit {
            rate: req.rate,
            burst: req.burst,
            direction: req.direction.clone(),
            policer_name,
            created: None,
        },
    );
    save_config(&config)?;

    info!(
        "Interface limit set on {}: {}, burst {}, direction {}",
        iface,
        format_rate(req.rate),
        req.burst,
        req.direction
    );

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Interface limit set on {}: {}, burst {}, direction {}", iface, format_rate(req.rate), req.burst, req.direction),
    }))
}

/// Remove per-interface bandwidth limit.
pub fn remove_interface_limit(iface: &str) -> Result<serde_json::Value> {
    let mut config = load_config();

    if !config.interface_limits.contains_key(iface) {
        anyhow::bail!("No traffic limit configured on {}", iface);
    }

    let limit = config.interface_limits.remove(iface).unwrap();

    let _ = run_vppctl(&["set", "interface", "input", "policer", iface, "0"]);
    let _ = run_vppctl(&["set", "interface", "output", "policer", iface, "0"]);
    let _ = run_vppctl(&["policer", "del", &limit.policer_name]);

    save_config(&config)?;

    info!("Interface limit removed from {}", iface);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Interface limit removed from {}", iface),
    }))
}

/// Set per-IP bandwidth limit via VPP policer.
pub fn set_ip_limit(req: SetIpLimitRequest) -> Result<serde_json::Value> {
    let mut config = load_config();
    let policer_name = format!("tc-ip-{}", req.ip.replace('.', "-"));

    // Remove existing
    if let Some(old) = config.ip_limits.get(&req.ip) {
        let _ = run_vppctl(&["policer", "del", &old.policer_name]);
    }

    // Create policer (rate in kbps)
    let rate_kbps = req.rate / 1000;
    let create_args = [
        "policer",
        "add",
        &policer_name,
        &rate_kbps.to_string(),
        &req.burst.to_string(),
        "type",
        "single_rate_two_color",
    ];
    run_vppctl(&create_args)?;

    config.ip_limits.insert(
        req.ip.clone(),
        IpLimit {
            rate: req.rate,
            burst: req.burst,
            policer_name,
            created: None,
        },
    );
    save_config(&config)?;

    info!(
        "IP limit set on {}: {}, burst {}",
        req.ip,
        format_rate(req.rate),
        req.burst
    );

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("IP limit set on {}: {}, burst {}", req.ip, format_rate(req.rate), req.burst),
    }))
}

/// Remove per-IP bandwidth limit.
pub fn remove_ip_limit(ip: &str) -> Result<serde_json::Value> {
    let mut config = load_config();

    if !config.ip_limits.contains_key(ip) {
        anyhow::bail!("No traffic limit configured for {}", ip);
    }

    let limit = config.ip_limits.remove(ip).unwrap();
    let _ = run_vppctl(&["policer", "del", &limit.policer_name]);

    save_config(&config)?;

    info!("IP limit removed for {}", ip);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("IP limit removed for {}", ip),
    }))
}

/// Set priority queue for a traffic class.
pub fn set_priority(req: SetPriorityRequest) -> Result<serde_json::Value> {
    let mut config = load_config();

    let (weight, dscp) = match req.queue.as_str() {
        "high" => (40u32, 46u32),
        "medium" => (35u32, 0u32),
        "low" => (25u32, 8u32),
        _ => anyhow::bail!("Invalid priority '{}'. Choose from: high, medium, low", req.queue),
    };

    config.priority_queues.insert(
        req.name.clone(),
        PriorityQueue {
            level: req.queue.clone(),
            weight,
            dscp,
            description: req.description,
        },
    );
    save_config(&config)?;

    info!("Priority for '{}' set to {}", req.name, req.queue);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Priority for '{}' set to {}", req.name, req.queue),
        "queue": config.priority_queues[&req.name],
    }))
}

/// Set application-based QoS class.
pub fn set_app_class(req: SetAppClassRequest) -> Result<serde_json::Value> {
    let mut config = load_config();

    let valid_priorities = ["high", "medium", "low"];
    if !valid_priorities.contains(&req.priority.as_str()) {
        anyhow::bail!(
            "Invalid priority '{}'. Choose from: high, medium, low",
            req.priority
        );
    }

    let dscp = match req.dscp {
        Some(d) => d,
        None => match req.priority.as_str() {
            "high" => 46u32,
            "medium" => 0u32,
            "low" => 8u32,
            _ => 0,
        },
    };

    config.app_classes.insert(
        req.name.clone(),
        AppClass {
            ports: req.ports,
            protocol: req.protocol,
            priority: req.priority,
            dscp,
            description: req.description,
        },
    );
    save_config(&config)?;

    info!("App class '{}' configured: DSCP={}", req.name, dscp);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("App class '{}' configured: DSCP={}", req.name, dscp),
    }))
}

/// Remove application-based QoS class.
pub fn remove_app_class(name: &str) -> Result<serde_json::Value> {
    let mut config = load_config();

    if !config.app_classes.contains_key(name) {
        anyhow::bail!("App class '{}' not found", name);
    }

    config.app_classes.remove(name);
    save_config(&config)?;

    info!("App class '{}' removed", name);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("App class '{}' removed", name),
    }))
}

/// Load default application QoS classes (gaming, video, voip, download).
pub fn load_defaults() -> Result<serde_json::Value> {
    let mut config = load_config();

    let defaults: Vec<(&str, &str, &str, &str, u32)> = vec![
        ("gaming", "3074,27015-27030,2005,3478-3480,3658", "udp", "high", 46),
        ("video", "443,80", "tcp", "high", 34),
        ("voip", "5060-5061,10000-20000", "udp", "high", 46),
        ("download", "80,443", "tcp", "low", 8),
    ];

    for (name, ports, protocol, priority, dscp) in &defaults {
        config.app_classes.insert(
            name.to_string(),
            AppClass {
                ports: ports.to_string(),
                protocol: protocol.to_string(),
                priority: priority.to_string(),
                dscp: *dscp,
                description: Some(format!("{} traffic", name)),
            },
        );
    }

    let count = defaults.len();
    save_config(&config)?;

    info!("Loaded {} default app classes", count);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Loaded {} default app classes", count),
        "classes": vec!["gaming", "video", "voip", "download"],
    }))
}

/// Configure burst control.
pub fn set_burst_control(req: SetBurstControlRequest) -> Result<serde_json::Value> {
    let mut config = load_config();

    config.burst_control = Some(BurstControl {
        enabled: req.enabled,
        default_burst_bytes: req.burst_bytes,
        max_burst_multiplier: req.max_burst_multiplier,
    });

    // Re-apply burst sizes to existing interface limits
    for (_iface, limit) in config.interface_limits.iter_mut() {
        let new_burst = std::cmp::min(
            req.burst_bytes,
            ((limit.rate / 8) as f64 * req.max_burst_multiplier) as u64,
        );
        limit.burst = new_burst;

        let rate_kbps = limit.rate / 1000;
        let _ = run_vppctl(&["policer", "del", &limit.policer_name]);
        let _ = run_vppctl(&[
            "policer",
            "add",
            &limit.policer_name,
            &rate_kbps.to_string(),
            &new_burst.to_string(),
            "type",
            "single_rate_two_color",
        ]);
    }

    save_config(&config)?;

    info!("Burst control configured: enabled={}", req.enabled);

    Ok(serde_json::json!({
        "status": "ok",
        "message": "Burst control configured",
        "burst_control": config.burst_control,
    }))
}

/// Get traffic statistics.
pub fn get_stats() -> Result<serde_json::Value> {
    let config = load_config();

    let vpp_policers = run_vppctl(&["show", "policer"]).unwrap_or_else(|_| "N/A".to_string());
    let vpp_policers_verbose =
        run_vppctl(&["show", "policer", "verbose"]).unwrap_or_else(|_| "N/A".to_string());
    let vpp_classify = run_vppctl(&["show", "classify", "table"]).unwrap_or_else(|_| "N/A".to_string());

    // Compute per-device usage summary from ip_limits
    let mut device_summary: Vec<serde_json::Value> = Vec::new();
    for (ip, limit) in &config.ip_limits {
        device_summary.push(serde_json::json!({
            "ip": ip,
            "rate_limit": format_rate(limit.rate),
            "rate_bps": limit.rate,
            "burst": limit.burst,
            "policer": limit.policer_name,
        }));
    }

    Ok(serde_json::json!({
        "status": "ok",
        "interface_limits": config.interface_limits,
        "ip_limits": config.ip_limits,
        "app_classes": config.app_classes,
        "priority_queues": config.priority_queues,
        "burst_control": config.burst_control,
        "global_enabled": config.global_enabled,
        "vpp_policers": vpp_policers,
        "vpp_policers_verbose": vpp_policers_verbose,
        "vpp_classify": vpp_classify,
        "device_summary": device_summary,
        "total_interface_limits": config.interface_limits.len(),
        "total_ip_limits": config.ip_limits.len(),
        "total_app_classes": config.app_classes.len(),
    }))
}

/// Reset all traffic control rules.
pub fn reset() -> Result<serde_json::Value> {
    let config = load_config();

    // Remove interface limits
    for (iface, limit) in &config.interface_limits {
        let _ = run_vppctl(&["set", "interface", "input", "policer", iface, "0"]);
        let _ = run_vppctl(&["set", "interface", "output", "policer", iface, "0"]);
        let _ = run_vppctl(&["policer", "del", &limit.policer_name]);
    }

    // Remove IP limits
    for (_ip, limit) in &config.ip_limits {
        let _ = run_vppctl(&["policer", "del", &limit.policer_name]);
    }

    // Reset config to defaults
    let new_config = TrafficConfig {
        global_enabled: true,
        priority_queues: HashMap::from([
            (
                "high".to_string(),
                PriorityQueue {
                    level: "high".to_string(),
                    weight: 40,
                    dscp: 46,
                    description: Some("Real-time / latency-sensitive".to_string()),
                },
            ),
            (
                "medium".to_string(),
                PriorityQueue {
                    level: "medium".to_string(),
                    weight: 35,
                    dscp: 0,
                    description: Some("Interactive / general traffic".to_string()),
                },
            ),
            (
                "low".to_string(),
                PriorityQueue {
                    level: "low".to_string(),
                    weight: 25,
                    dscp: 8,
                    description: Some("Bulk / background traffic".to_string()),
                },
            ),
        ]),
        ..TrafficConfig::default()
    };

    save_config(&new_config)?;

    info!("All traffic control rules reset");

    Ok(serde_json::json!({
        "status": "ok",
        "message": "All traffic control rules removed",
    }))
}
