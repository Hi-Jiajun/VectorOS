//! DNS forwarding service
//!
//! Manages dnsmasq as a DNS forwarder. Handles upstream server configuration,
//! cache settings, and merges with existing DHCP config when both are enabled.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

const CONFIG_PATH: &str = "/etc/vectoros-dhcp.conf";

/// DNS service status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStatus {
    pub status: String,
    pub upstream: Vec<String>,
    pub upstream_v6: Vec<String>,
    pub cache_size: u32,
    pub interface: String,
}

/// Configuration for enabling DNS forwarding
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct DnsEnableConfig {
    #[serde(default = "default_upstream")]
    pub upstream: String,
    #[serde(default = "default_upstream_v6")]
    pub upstream_v6: String,
    #[serde(default = "default_interface")]
    pub interface: String,
    #[serde(default = "default_cache_size")]
    pub cache_size: u32,
}

fn default_upstream() -> String {
    "8.8.8.8,1.1.1.1".to_string()
}
fn default_upstream_v6() -> String {
    "2001:4860:4860::8888,2606:4700:4700::1111".to_string()
}
fn default_interface() -> String {
    "lan0".to_string()
}
fn default_cache_size() -> u32 {
    1000
}

/// Check if dnsmasq is running.
pub fn is_dnsmasq_running() -> bool {
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

/// Read existing DHCP config lines (dhcp-range, dhcp-option) from the shared config file.
fn read_dhcp_lines() -> Vec<String> {
    let path = Path::new(CONFIG_PATH);
    if !path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            (lower.contains("dhcp") || lower.contains("bind-dynamic"))
                && !line.trim().starts_with('#')
        })
        .map(|l| l.to_string())
        .collect()
}

/// Enable DNS forwarding using dnsmasq.
///
/// Merges DNS config with any existing DHCP config in the shared config file.
pub fn enable(config: DnsEnableConfig) -> Result<serde_json::Value> {
    kill_dnsmasq();

    // Build upstream DNS server lines (IPv4)
    let upstream_lines: Vec<String> = config
        .upstream
        .split(',')
        .map(|s| format!("server={}", s.trim()))
        .collect();

    // Build upstream DNS server lines (IPv6)
    let upstream_v6_lines: Vec<String> = config
        .upstream_v6
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("server={}", s.trim()))
        .collect();

    let mut dnsmasq_config = format!(
        "# VectorOS DNS Configuration\n\
         {upstream_v4}\n\
         {upstream_v6}\n\
         cache-size={cache_size}\n\
         listen-address=127.0.0.1,192.168.1.1\n\
         interface={interface}\n\
         bind-dynamic\n\
         no-resolv\n\
         no-poll\n\
         log-queries\n",
        upstream_v4 = upstream_lines.join("\n"),
        upstream_v6 = upstream_v6_lines.join("\n"),
        cache_size = config.cache_size,
        interface = config.interface,
    );

    // Merge DHCP lines if present
    let dhcp_lines = read_dhcp_lines();
    if !dhcp_lines.is_empty() {
        dnsmasq_config.push('\n');
        dnsmasq_config.push_str(&dhcp_lines.join("\n"));
        dnsmasq_config.push('\n');
    }

    fs::write(CONFIG_PATH, &dnsmasq_config)
        .context("Failed to write DNS config file")?;

    // Start dnsmasq
    Command::new("dnsmasq")
        .arg(format!("--conf-file={}", CONFIG_PATH))
        .spawn()
        .context("Failed to start dnsmasq")?;

    std::thread::sleep(std::time::Duration::from_secs(1));

    if is_dnsmasq_running() {
        info!("DNS forwarding enabled");
        Ok(serde_json::json!({
            "status": "ok",
            "message": "DNS forwarding enabled"
        }))
    } else {
        warn!("Failed to start dnsmasq for DNS");
        Ok(serde_json::json!({
            "error": "Failed to start dnsmasq"
        }))
    }
}

/// Disable DNS forwarding.
pub fn disable() -> Result<serde_json::Value> {
    kill_dnsmasq();

    let _ = fs::remove_file(CONFIG_PATH);

    info!("DNS forwarding disabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "DNS forwarding disabled"
    }))
}

/// Show DNS forwarding status and configuration.
pub fn show() -> Result<serde_json::Value> {
    let status = if is_dnsmasq_running() {
        "active"
    } else {
        "inactive"
    };

    let mut result = DnsStatus {
        status: status.to_string(),
        upstream: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
        upstream_v6: vec![
            "2001:4860:4860::8888".to_string(),
            "2606:4700:4700::1111".to_string(),
        ],
        cache_size: 1000,
        interface: "lan0".to_string(),
    };

    // Parse config file for actual values
    let path = Path::new(CONFIG_PATH);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            let mut ipv4_servers = Vec::new();
            let mut ipv6_servers = Vec::new();

            for line in content.lines() {
                if let Some(server) = line.strip_prefix("server=") {
                    let server = server.trim().to_string();
                    if server.contains(':') {
                        ipv6_servers.push(server);
                    } else {
                        ipv4_servers.push(server);
                    }
                } else if let Some(val) = line.strip_prefix("cache-size=") {
                    if let Ok(size) = val.trim().parse::<u32>() {
                        result.cache_size = size;
                    }
                } else if let Some(val) = line.strip_prefix("interface=") {
                    result.interface = val.trim().to_string();
                }
            }

            if !ipv4_servers.is_empty() {
                result.upstream = ipv4_servers;
            }
            if !ipv6_servers.is_empty() {
                result.upstream_v6 = ipv6_servers;
            }
        }
    }

    Ok(serde_json::json!(result))
}
