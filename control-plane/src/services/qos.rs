//! QoS management service
//!
//! Manages VPP policers, rate limiting, and DSCP marking.
//! Persists configuration to a JSON file and applies via vppctl.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

const QOS_FILE: &str = "/etc/vectoros/qos-config.json";

/// Stored QoS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QosConfig {
    #[serde(default)]
    pub policers: HashMap<String, PolicerConfig>,
    #[serde(default)]
    pub rate_limits: HashMap<String, RateLimitConfig>,
    #[serde(default)]
    pub dscp_marks: Vec<DscpMarkRule>,
}

/// A policer definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicerConfig {
    pub rate: u64,
    pub burst: u64,
    #[serde(rename = "type")]
    pub policer_type: String,
    #[serde(default)]
    pub interfaces: Vec<String>,
}

/// Per-interface rate limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub rate: u64,
    pub burst: u64,
    pub direction: String,
    pub policer_name: String,
}

/// DSCP marking rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DscpMarkRule {
    pub dscp: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_port: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_port: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to create a policer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicerRequest {
    pub name: String,
    pub rate: u64,
    pub burst: u64,
    #[serde(default = "default_policer_type")]
    pub policer_type: String,
}

fn default_policer_type() -> String {
    "single_rate_two_color".to_string()
}

/// Request to set interface rate limit
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

/// Load saved QoS config from disk.
fn load_config() -> QosConfig {
    let path = Path::new(QOS_FILE);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<QosConfig>(&content) {
                return data;
            }
        }
    }
    QosConfig::default()
}

/// Persist QoS config to disk.
fn save_config(config: &QosConfig) -> Result<()> {
    let path = Path::new(QOS_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create QoS config directory")?;
    }
    let json = serde_json::to_string_pretty(config).context("Failed to serialize QoS config")?;
    fs::write(path, json).context("Failed to write QoS config file")?;
    Ok(())
}

/// Create a policer in VPP.
pub fn create_policer(req: CreatePolicerRequest) -> Result<serde_json::Value> {
    let mut config = load_config();

    let args = [
        "policer",
        "add",
        &req.name,
        &req.rate.to_string(),
        &req.burst.to_string(),
        "type",
        &req.policer_type,
    ];

    run_vppctl(&args)?;

    config.policers.insert(
        req.name.clone(),
        PolicerConfig {
            rate: req.rate,
            burst: req.burst,
            policer_type: req.policer_type,
            interfaces: Vec::new(),
        },
    );

    save_config(&config)?;

    info!("Policer '{}' created: rate={} bps, burst={}", req.name, req.rate, req.burst);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Policer '{}' created", req.name),
        "policer": config.policers[&req.name],
    }))
}

/// Delete a policer from VPP.
pub fn delete_policer(name: &str) -> Result<serde_json::Value> {
    let mut config = load_config();

    if !config.policers.contains_key(name) {
        anyhow::bail!("Policer '{}' not found", name);
    }

    // Remove from all interfaces first
    if let Some(policer) = config.policers.get(name) {
        for iface in &policer.interfaces {
            let _ = run_vppctl(&["set", "interface", "policer", iface, "0"]);
        }
    }

    run_vppctl(&["policer", "del", name])?;

    config.policers.remove(name);
    save_config(&config)?;

    info!("Policer '{}' deleted", name);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Policer '{}' deleted", name),
    }))
}

/// Set rate limit on an interface.
pub fn set_interface_limit(
    iface: &str,
    req: SetInterfaceLimitRequest,
) -> Result<serde_json::Value> {
    let mut config = load_config();
    let policer_name = format!("{}-limit", iface);

    // Remove existing limit if any
    let _ = run_vppctl(&["set", "interface", "policer", iface, "0"]);
    let _ = run_vppctl(&["policer", "del", &policer_name]);

    // Create the policer
    let create_args = [
        "policer",
        "add",
        &policer_name,
        &req.rate.to_string(),
        &req.burst.to_string(),
        "type",
        "single_rate_two_color",
    ];
    run_vppctl(&create_args)?;

    // Apply to interface
    let apply_args: Vec<&str> = match req.direction.as_str() {
        "input" => vec!["set", "interface", "input", "policer", iface, &policer_name],
        "output" => vec!["set", "interface", "output", "policer", iface, &policer_name],
        _ => vec!["set", "interface", "policer", iface, &policer_name],
    };
    run_vppctl(&apply_args)?;

    config.rate_limits.insert(
        iface.to_string(),
        RateLimitConfig {
            rate: req.rate,
            burst: req.burst,
            direction: req.direction.clone(),
            policer_name: policer_name.clone(),
        },
    );

    // Also register as a policer
    config.policers.insert(
        policer_name.clone(),
        PolicerConfig {
            rate: req.rate,
            burst: req.burst,
            policer_type: "single_rate_two_color".to_string(),
            interfaces: vec![iface.to_string()],
        },
    );

    save_config(&config)?;

    info!(
        "Rate limit set on {}: {} bps, burst {}, direction {}",
        iface, req.rate, req.burst, req.direction
    );

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Rate limit set on {}: {} bps, burst {}, direction {}", iface, req.rate, req.burst, req.direction),
    }))
}

/// Remove rate limit from an interface.
pub fn remove_interface_limit(iface: &str) -> Result<serde_json::Value> {
    let mut config = load_config();

    if !config.rate_limits.contains_key(iface) {
        anyhow::bail!("No rate limit configured on {}", iface);
    }

    let limit = config.rate_limits.remove(iface).unwrap();

    let _ = run_vppctl(&["set", "interface", "policer", iface, "0"]);
    let _ = run_vppctl(&["policer", "del", &limit.policer_name]);
    config.policers.remove(&limit.policer_name);

    save_config(&config)?;

    info!("Rate limit removed from {}", iface);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Rate limit removed from {}", iface),
    }))
}

/// Show full QoS status.
pub fn show_status() -> Result<serde_json::Value> {
    let config = load_config();

    let vpp_policer_output = run_vppctl(&["show", "policer"]).unwrap_or_else(|_| "N/A".to_string());

    Ok(serde_json::json!({
        "status": "ok",
        "policers": config.policers,
        "rate_limits": config.rate_limits,
        "dscp_marks": config.dscp_marks,
        "vpp_policer_output": vpp_policer_output,
        "total_policers": config.policers.len(),
        "total_rate_limits": config.rate_limits.len(),
        "total_dscp_marks": config.dscp_marks.len(),
    }))
}
