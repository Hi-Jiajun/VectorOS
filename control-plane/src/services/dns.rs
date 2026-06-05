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

/// Clear the DNS cache by sending SIGHUP to dnsmasq (causes cache flush).
pub fn clear_cache() -> Result<serde_json::Value> {
    if !is_dnsmasq_running() {
        return Ok(serde_json::json!({
            "error": "DNS service is not running"
        }));
    }

    // SIGHUP causes dnsmasq to reload config and flush cache
    let output = Command::new("kill")
        .arg("-HUP")
        .arg(
            Command::new("pgrep")
                .arg("dnsmasq")
                .output()
                .context("Failed to get dnsmasq PID")?
                .stdout
                .to_vec()
                .iter()
                .filter(|b| b.is_ascii_digit())
                .map(|b| *b as char)
                .collect::<String>(),
        )
        .output()
        .context("Failed to send SIGHUP to dnsmasq")?;

    if output.status.success() {
        info!("DNS cache cleared");
        Ok(serde_json::json!({
            "status": "ok",
            "message": "DNS cache cleared"
        }))
    } else {
        Ok(serde_json::json!({
            "error": "Failed to clear DNS cache"
        }))
    }
}

/// Get DNS cache statistics by querying dnsmasq via its statistics.
pub fn get_cache_stats() -> Result<serde_json::Value> {
    let running = is_dnsmasq_running();

    // Try to read dnsmasq stats from the config or generate basic stats
    let mut stats = serde_json::json!({
        "running": running,
        "hits": 0,
        "misses": 0,
        "insertions": 0,
        "evictions": 0,
        "size": 0,
    });

    // If dnsmasq is running, try to get cache stats via --bind-interfaces stats
    // dnsmasq doesn't expose stats over a socket by default, so we parse the log
    // or use the cache-size from config as the max
    if running {
        let path = Path::new(CONFIG_PATH);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                for line in content.lines() {
                    if let Some(val) = line.strip_prefix("cache-size=") {
                        if let Ok(size) = val.trim().parse::<u32>() {
                            stats["size"] = serde_json::json!(size);
                        }
                    }
                }
            }
        }

        // Try to read dnsmasq log for cache statistics
        let log_paths = ["/var/log/dnsmasq.log", "/tmp/dnsmasq.log"];
        for log_path in &log_paths {
            let log_file = Path::new(log_path);
            if log_file.exists() {
                if let Ok(content) = fs::read_to_string(log_file) {
                    for line in content.lines() {
                        if line.contains("cached") {
                            if let Some(val) = stats["hits"].as_u64() {
                                stats["hits"] = serde_json::json!(val + 1);
                            }
                        } else if line.contains("uncached") || line.contains("forwarded") {
                            if let Some(val) = stats["misses"].as_u64() {
                                stats["misses"] = serde_json::json!(val + 1);
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    Ok(stats)
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
