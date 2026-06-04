use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub vpp: VppConfig,
    pub network: NetworkConfig,
    pub dhcp: DhcpConfig,
    pub dns: DnsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppConfig {
    /// VPP binary API socket path
    #[serde(default = "default_vpp_socket")]
    pub socket_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// WAN interface name
    pub wan_interface: Option<String>,
    /// LAN interface name
    pub lan_interface: Option<String>,
    /// PPPoE config
    pub pppoe: Option<PppoeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeConfig {
    pub username: String,
    pub password: String,
    pub interface: String,
    /// Auto-connect configuration
    #[serde(default)]
    pub autoconnect: Option<PppoeAutoConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeAutoConfig {
    /// Whether auto-connect is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Maximum retries before giving up. 0 = infinite.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial retry interval in seconds.
    #[serde(default = "default_retry_interval")]
    pub retry_interval: u64,
    /// Exponential backoff multiplier.
    #[serde(default = "default_backoff_factor")]
    pub backoff_factor: f64,
    /// Maximum retry interval cap in seconds.
    #[serde(default = "default_max_retry_interval")]
    pub max_retry_interval: u64,
    /// Interval between connection status checks in seconds.
    #[serde(default = "default_check_interval")]
    pub check_interval: u64,
    /// Interval between health checks while connected in seconds.
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,
}

fn default_max_retries() -> u32 { 0 }
fn default_retry_interval() -> u64 { 5 }
fn default_backoff_factor() -> f64 { 2.0 }
fn default_max_retry_interval() -> u64 { 300 }
fn default_check_interval() -> u64 { 10 }
fn default_health_check_interval() -> u64 { 60 }

impl Default for PppoeAutoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_retries: default_max_retries(),
            retry_interval: default_retry_interval(),
            backoff_factor: default_backoff_factor(),
            max_retry_interval: default_max_retry_interval(),
            check_interval: default_check_interval(),
            health_check_interval: default_health_check_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: Option<String>,
    pub range_end: Option<String>,
    pub lease_time: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub upstream: Vec<String>,
    pub cache_size: Option<usize>,
}

fn default_vpp_socket() -> String {
    "/run/vpp/api.sock".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vpp: VppConfig {
                socket_path: default_vpp_socket(),
            },
            network: NetworkConfig {
                wan_interface: None,
                lan_interface: None,
                pppoe: None,
            },
            dhcp: DhcpConfig {
                enabled: false,
                range_start: None,
                range_end: None,
                lease_time: None,
            },
            dns: DnsConfig {
                upstream: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
                cache_size: Some(1000),
            },
        }
    }
}

pub fn load(path: &str) -> Result<Config, anyhow::Error> {
    let content = std::fs::read_to_string(Path::new(path))?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}
