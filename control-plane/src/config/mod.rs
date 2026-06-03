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
