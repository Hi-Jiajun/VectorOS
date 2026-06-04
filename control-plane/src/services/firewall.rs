//! Firewall management service
//!
//! Manages VPP ACL-based firewall rules. Persists rules to a JSON file
//! and applies them to VPP via vppctl commands.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

const RULES_FILE: &str = "/etc/vectoros/firewall-rules.json";

/// Stored firewall rule set
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallData {
    pub enabled: bool,
    pub rules: Vec<FirewallRule>,
}

/// A single firewall rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub id: u32,
    pub action: String,
    pub protocol: Option<String>,
    pub enabled: bool,
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

/// Request to add a firewall rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRuleRequest {
    pub action: String,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    #[serde(default)]
    pub src_port: Option<u32>,
    #[serde(default)]
    pub dst_port: Option<u32>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
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

/// Load saved rules from disk.
fn load_rules() -> FirewallData {
    let path = Path::new(RULES_FILE);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<FirewallData>(&content) {
                return data;
            }
        }
    }
    FirewallData {
        enabled: true,
        rules: Vec::new(),
    }
}

/// Persist rules to disk.
fn save_rules(data: &FirewallData) -> Result<()> {
    let path = Path::new(RULES_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create firewall rules directory")?;
    }
    let json = serde_json::to_string_pretty(data).context("Failed to serialize firewall rules")?;
    fs::write(path, json).context("Failed to write firewall rules file")?;
    Ok(())
}

/// Apply the full rule set to VPP via vppctl ACL commands.
fn apply_rules_to_vpp(rules: &[FirewallRule]) -> Result<Vec<serde_json::Value>> {
    // Enable the ACL plugin (ignore error if already enabled)
    let _ = run_vppctl(&["acl", "plugin", "enable"]);

    let mut results = Vec::new();

    for rule in rules {
        if !rule.enabled {
            continue;
        }

        let mut cmd_parts = vec!["acl".to_string(), "add".to_string()];

        if let Some(ref src_ip) = rule.src_ip {
            cmd_parts.push(format!("src-ip {}", src_ip));
        }
        if let Some(ref dst_ip) = rule.dst_ip {
            cmd_parts.push(format!("dst-ip {}", dst_ip));
        }
        if let Some(dst_port) = rule.dst_port {
            cmd_parts.push(format!("dst-port {}", dst_port));
        }

        match rule.action.as_str() {
            "permit" => cmd_parts.push("action permit".to_string()),
            _ => cmd_parts.push("action deny".to_string()),
        }

        let args: Vec<&str> = cmd_parts.iter().map(|s| s.as_str()).collect();
        let out = run_vppctl(&args);
        let (stdout, stderr, success) = match out {
            Ok(s) => (s, String::new(), true),
            Err(e) => (String::new(), e.to_string(), false),
        };

        results.push(serde_json::json!({
            "entry": cmd_parts.join(" "),
            "stdout": stdout,
            "stderr": stderr,
            "rc": if success { 0 } else { 1 },
        }));
    }

    Ok(results)
}

/// Add a firewall rule.
pub fn add_rule(req: AddRuleRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let next_id = data
        .rules
        .iter()
        .map(|r| r.id)
        .max()
        .unwrap_or(0)
        + 1;

    let new_rule = FirewallRule {
        id: next_id,
        action: req.action,
        protocol: req.protocol.or_else(|| Some("ip".to_string())),
        enabled: true,
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        description: req.description,
    };

    data.rules.push(new_rule.clone());
    save_rules(&data)?;

    // Apply to VPP if firewall is enabled
    if data.enabled {
        apply_rules_to_vpp(&data.rules)?;
    }

    let total_rules = data.rules.len();
    info!("Firewall rule {} added, total: {}", new_rule.id, total_rules);

    Ok(serde_json::json!({
        "status": "ok",
        "rule": new_rule,
        "total_rules": total_rules,
    }))
}

/// Delete a firewall rule by ID.
pub fn del_rule(id: u32) -> Result<serde_json::Value> {
    let mut data = load_rules();
    let original_count = data.rules.len();

    data.rules.retain(|r| r.id != id);

    if data.rules.len() == original_count {
        anyhow::bail!("Rule with id {} not found", id);
    }

    save_rules(&data)?;

    if data.enabled {
        apply_rules_to_vpp(&data.rules)?;
    }

    let total_rules = data.rules.len();
    info!("Firewall rule {} deleted, total: {}", id, total_rules);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Rule {} deleted", id),
        "total_rules": total_rules,
    }))
}

/// Show current firewall rules and status.
pub fn show() -> Result<serde_json::Value> {
    let data = load_rules();
    let active_rules = data.rules.iter().filter(|r| r.enabled).count();

    // Get VPP ACL status
    let vpp_acl_status = run_vppctl(&["show", "acl"]).unwrap_or_else(|_| "N/A".to_string());

    Ok(serde_json::json!({
        "status": "ok",
        "enabled": data.enabled,
        "rules": data.rules,
        "total_rules": data.rules.len(),
        "active_rules": active_rules,
        "vpp_acl_status": vpp_acl_status,
    }))
}

/// Enable the firewall and apply all active rules.
pub fn enable() -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.enabled = true;
    save_rules(&data)?;

    let _ = run_vppctl(&["acl", "plugin", "enable"]);
    apply_rules_to_vpp(&data.rules)?;

    info!("Firewall enabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "Firewall enabled"
    }))
}

/// Disable the firewall.
pub fn disable() -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.enabled = false;
    save_rules(&data)?;

    let _ = run_vppctl(&["acl", "plugin", "disable"]);

    info!("Firewall disabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "Firewall disabled"
    }))
}
