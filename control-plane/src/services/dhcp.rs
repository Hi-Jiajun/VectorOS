//! DHCP management service
//!
//! Manages dnsmasq as a DHCP server without Python dependencies.
//! Handles process lifecycle, config file generation, and lease parsing.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

const DHCP_CONFIG_PATH: &str = "/etc/vectoros-dhcp.conf";
const LEASE_FILE_PATH: &str = "/var/lib/misc/dnsmasq.leases";

/// DHCP server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpStatus {
    pub status: String,
    pub leases: Vec<DhcpLease>,
}

/// A single DHCP lease entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpLease {
    pub mac: String,
    pub ip: String,
    pub hostname: String,
    pub expires: String,
}

/// Configuration for enabling the DHCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct DhcpEnableConfig {
    #[serde(default = "default_interface")]
    pub interface: String,
    #[serde(default = "default_start_ip")]
    pub start_ip: String,
    #[serde(default = "default_end_ip")]
    pub end_ip: String,
    #[serde(default = "default_gateway")]
    pub gateway: String,
    #[serde(default = "default_lease_time")]
    pub lease_time: u32,
    #[serde(default)]
    pub dns_servers: Option<String>,
}

fn default_interface() -> String {
    "lan0".to_string()
}
fn default_start_ip() -> String {
    "192.168.1.100".to_string()
}
fn default_end_ip() -> String {
    "192.168.1.200".to_string()
}
fn default_gateway() -> String {
    "192.168.1.1".to_string()
}
fn default_lease_time() -> u32 {
    86400
}

/// Check if dnsmasq is running by looking for its process.
fn is_dnsmasq_running() -> bool {
    Command::new("pgrep")
        .arg("dnsmasq")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Kill all running dnsmasq instances.
fn kill_dnsmasq() {
    let _ = Command::new("pkill")
        .arg("-9")
        .arg("dnsmasq")
        .output();
}

/// Enable the DHCP server.
///
/// Writes a dnsmasq config file and starts the process.
pub fn enable(config: DhcpEnableConfig) -> Result<serde_json::Value> {
    kill_dnsmasq();

    let dns_servers = config
        .dns_servers
        .as_deref()
        .unwrap_or("8.8.8.8,1.1.1.1");

    let dnsmasq_config = format!(
        "interface={interface}\n\
         bind-dynamic\n\
         dhcp-range={start_ip},{end_ip},{lease_time}s\n\
         dhcp-option=option:router,{gateway}\n\
         dhcp-option=option:dns-server,{dns_servers}\n\
         log-dhcp\n",
        interface = config.interface,
        start_ip = config.start_ip,
        end_ip = config.end_ip,
        lease_time = config.lease_time,
        gateway = config.gateway,
        dns_servers = dns_servers,
    );

    fs::write(DHCP_CONFIG_PATH, &dnsmasq_config)
        .context("Failed to write DHCP config file")?;

    // Start dnsmasq
    Command::new("dnsmasq")
        .arg(format!("--conf-file={}", DHCP_CONFIG_PATH))
        .spawn()
        .context("Failed to start dnsmasq")?;

    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_secs(1));

    if is_dnsmasq_running() {
        info!("DHCP server enabled on {}", config.interface);
        Ok(serde_json::json!({
            "status": "ok",
            "message": "DHCP server enabled"
        }))
    } else {
        warn!("Failed to start dnsmasq");
        Ok(serde_json::json!({
            "error": "Failed to start dnsmasq"
        }))
    }
}

/// Disable the DHCP server.
pub fn disable() -> Result<serde_json::Value> {
    kill_dnsmasq();

    let _ = fs::remove_file(DHCP_CONFIG_PATH);

    info!("DHCP server disabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "DHCP server disabled"
    }))
}

/// Show DHCP server status and current leases.
pub fn show() -> Result<serde_json::Value> {
    let status = if is_dnsmasq_running() {
        "active"
    } else {
        "inactive"
    };

    let leases = read_leases()?;

    Ok(serde_json::json!({
        "status": status,
        "leases": leases,
    }))
}

/// Parse the dnsmasq lease file.
fn read_leases() -> Result<Vec<DhcpLease>> {
    let path = Path::new(LEASE_FILE_PATH);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).context("Failed to read lease file")?;
    let mut leases = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            leases.push(DhcpLease {
                expires: parts[0].to_string(),
                mac: parts[1].to_string(),
                ip: parts[2].to_string(),
                hostname: parts[3].to_string(),
            });
        }
    }

    Ok(leases)
}
