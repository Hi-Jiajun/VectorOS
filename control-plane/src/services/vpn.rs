//! VPN management service
//!
//! Manages WireGuard, IPsec, and OpenVPN tunnels.
//! Delegates to the Python vpn_manager.py helper for VPP/kernel operations
//! and persists configuration to a JSON file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

const VPN_CONFIG_FILE: &str = "/etc/vectoros/vpn/state.json";
const VPN_MANAGER: &str = "/root/VectorOS/vpp-tools/vpn_manager.py";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Top-level VPN state persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VpnState {
    #[serde(default)]
    pub wireguard: std::collections::HashMap<String, WireGuardConfig>,
    #[serde(default)]
    pub ipsec: std::collections::HashMap<String, IpsecConfig>,
    #[serde(default)]
    pub openvpn: std::collections::HashMap<String, OpenVpnConfig>,
}

/// WireGuard tunnel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub name: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub peer_endpoint: String,
    #[serde(default)]
    pub peer_public_key: String,
    #[serde(default = "default_allowed_ips")]
    pub peer_allowed_ips: String,
    #[serde(default)]
    pub dns: String,
    #[serde(default = "default_wg_mtu")]
    pub mtu: u16,
    #[serde(default)]
    pub pre_shared_key: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

fn default_listen_port() -> u16 { 51820 }
fn default_allowed_ips() -> String { "0.0.0.0/0".to_string() }
fn default_wg_mtu() -> u16 { 1420 }

/// IPsec tunnel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpsecConfig {
    pub name: String,
    #[serde(default = "default_ipsec_mode")]
    pub mode: String,
    #[serde(default = "default_ipsec_proto")]
    pub proto: String,
    #[serde(default)]
    pub local_ip: String,
    #[serde(default)]
    pub remote_ip: String,
    #[serde(default)]
    pub local_subnet: String,
    #[serde(default)]
    pub remote_subnet: String,
    #[serde(default)]
    pub local_id: String,
    #[serde(default)]
    pub remote_id: String,
    #[serde(default = "default_encryption")]
    pub encryption: String,
    #[serde(default = "default_integrity")]
    pub integrity: String,
    #[serde(default = "default_dh_group")]
    pub dh_group: String,
    #[serde(default = "default_ikelifetime")]
    pub ikelifetime: String,
    #[serde(default = "default_salifetime")]
    pub salifetime: String,
    #[serde(default)]
    pub pre_shared_key: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

fn default_ipsec_mode() -> String { "tunnel".to_string() }
fn default_ipsec_proto() -> String { "esp".to_string() }
fn default_encryption() -> String { "aes-256-gcm".to_string() }
fn default_integrity() -> String { "sha256".to_string() }
fn default_dh_group() -> String { "2".to_string() }
fn default_ikelifetime() -> String { "8h".to_string() }
fn default_salifetime() -> String { "1h".to_string() }

/// OpenVPN tunnel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenVpnConfig {
    pub name: String,
    #[serde(default = "default_ovpn_mode")]
    pub mode: String,
    #[serde(default)]
    pub remote: String,
    #[serde(default = "default_ovpn_port")]
    pub port: u16,
    #[serde(default = "default_ovpn_proto")]
    pub proto: String,
    #[serde(default)]
    pub ca_cert: String,
    #[serde(default)]
    pub client_cert: String,
    #[serde(default)]
    pub client_key: String,
    #[serde(default)]
    pub tls_auth: String,
    #[serde(default = "default_ovpn_device")]
    pub device: String,
    #[serde(default = "default_ovpn_cipher")]
    pub cipher: String,
    #[serde(default = "default_ovpn_auth")]
    pub auth: String,
    #[serde(default)]
    pub redirect_gateway: bool,
    #[serde(default)]
    pub dns_push: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

fn default_ovpn_mode() -> String { "client".to_string() }
fn default_ovpn_port() -> u16 { 1194 }
fn default_ovpn_proto() -> String { "udp".to_string() }
fn default_ovpn_device() -> String { "tun".to_string() }
fn default_ovpn_cipher() -> String { "AES-256-GCM".to_string() }
fn default_ovpn_auth() -> String { "SHA256".to_string() }

// ---------------------------------------------------------------------------
// Request types for API
// ---------------------------------------------------------------------------

/// Request to configure a WireGuard tunnel.
#[derive(Debug, Clone, Deserialize)]
pub struct WireGuardConfigRequest {
    pub name: Option<String>,
    pub listen_port: Option<u16>,
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub address: String,
    pub peer_endpoint: Option<String>,
    pub peer_public_key: Option<String>,
    pub peer_allowed_ips: Option<String>,
    pub pre_shared_key: Option<String>,
    pub dns: Option<String>,
    pub mtu: Option<u16>,
}

/// Request to configure an IPsec tunnel.
#[derive(Debug, Clone, Deserialize)]
pub struct IpsecConfigRequest {
    pub name: Option<String>,
    pub mode: Option<String>,
    pub proto: Option<String>,
    pub local_ip: String,
    pub remote_ip: String,
    pub local_subnet: Option<String>,
    pub remote_subnet: Option<String>,
    pub local_id: Option<String>,
    pub remote_id: Option<String>,
    pub encryption: Option<String>,
    pub integrity: Option<String>,
    pub dh_group: Option<String>,
    pub ikelifetime: Option<String>,
    pub salifetime: Option<String>,
    pub pre_shared_key: Option<String>,
}

/// Request to configure OpenVPN.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenVpnConfigRequest {
    pub name: Option<String>,
    pub mode: Option<String>,
    pub remote: String,
    pub port: Option<u16>,
    pub proto: Option<String>,
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub tls_auth: Option<String>,
    pub device: Option<String>,
    pub cipher: Option<String>,
    pub auth: Option<String>,
    pub redirect_gateway: Option<bool>,
    pub dns_push: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_state() -> VpnState {
    let path = Path::new(VPN_CONFIG_FILE);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<VpnState>(&content) {
                return data;
            }
        }
    }
    VpnState::default()
}

fn save_state(state: &VpnState) -> Result<()> {
    let path = Path::new(VPN_CONFIG_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create VPN config directory")?;
    }
    let json = serde_json::to_string_pretty(state).context("Failed to serialize VPN state")?;
    fs::write(path, json).context("Failed to write VPN state file")?;
    Ok(())
}

/// Run the Python VPN manager and return parsed JSON output.
fn run_vpn_manager(args: &[String]) -> Result<serde_json::Value> {
    let output = Command::new("python3")
        .arg(VPN_MANAGER)
        .args(args)
        .output()
        .context("Failed to run VPN manager")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("VPN manager failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse VPN manager output: {}", stdout))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Get overall VPN status.
pub fn get_status() -> Result<serde_json::Value> {
    match run_vpn_manager(&["status".to_string()]) {
        Ok(data) => Ok(data),
        Err(_) => {
            // Fallback: return state from local file
            let state = load_state();
            Ok(serde_json::json!({
                "status": "ok",
                "backends": {
                    "wireguard_kernel": false,
                    "wireguard_vpp": false,
                    "ipsec_kernel": false,
                    "ipsec_vpp": false,
                    "openvpn": false,
                },
                "wireguard": {
                    "tunnels": [],
                    "count": 0,
                    "configured": state.wireguard,
                },
                "ipsec": {
                    "security_associations": [],
                    "count": 0,
                    "configured": state.ipsec,
                },
                "openvpn": {
                    "connections": [],
                    "count": 0,
                    "configured": state.openvpn,
                },
            }))
        }
    }
}

/// List all active VPN connections.
pub fn list_connections() -> Result<serde_json::Value> {
    match run_vpn_manager(&["connections".to_string()]) {
        Ok(data) => Ok(data),
        Err(e) => Ok(serde_json::json!({
            "status": "ok",
            "connections": [],
            "total": 0,
            "error": e.to_string(),
        })),
    }
}

/// Configure a WireGuard tunnel.
pub fn configure_wireguard(req: WireGuardConfigRequest) -> Result<serde_json::Value> {
    let name = req.name.unwrap_or_else(|| "wg0".to_string());

    let mut args = vec![
        "wg".to_string(), "config".to_string(),
        "--name".to_string(), name.clone(),
    ];

    if let Some(port) = req.listen_port {
        args.push("--listen-port".to_string());
        args.push(port.to_string());
    }
    if let Some(ref key) = req.private_key {
        args.push("--private-key".to_string());
        args.push(key.clone());
    }
    if let Some(ref key) = req.public_key {
        args.push("--public-key".to_string());
        args.push(key.clone());
    }
    args.push("--address".to_string());
    args.push(req.address.clone());
    if let Some(ref ep) = req.peer_endpoint {
        args.push("--peer-endpoint".to_string());
        args.push(ep.clone());
    }
    if let Some(ref pk) = req.peer_public_key {
        args.push("--peer-public-key".to_string());
        args.push(pk.clone());
    }
    if let Some(ref ips) = req.peer_allowed_ips {
        args.push("--peer-allowed-ips".to_string());
        args.push(ips.clone());
    }
    if let Some(ref psk) = req.pre_shared_key {
        args.push("--pre-shared-key".to_string());
        args.push(psk.clone());
    }
    if let Some(ref dns) = req.dns {
        args.push("--dns".to_string());
        args.push(dns.clone());
    }
    if let Some(mtu) = req.mtu {
        args.push("--mtu".to_string());
        args.push(mtu.to_string());
    }

    let result = run_vpn_manager(&args)?;

    // Also store in local state
    let mut state = load_state();
    state.wireguard.insert(
        name.clone(),
        WireGuardConfig {
            name: name.clone(),
            listen_port: req.listen_port.unwrap_or(51820),
            private_key: req.private_key.unwrap_or_default(),
            public_key: req.public_key.unwrap_or_default(),
            address: req.address,
            peer_endpoint: req.peer_endpoint.unwrap_or_default(),
            peer_public_key: req.peer_public_key.unwrap_or_default(),
            peer_allowed_ips: req.peer_allowed_ips.unwrap_or_else(default_allowed_ips),
            pre_shared_key: req.pre_shared_key.unwrap_or_default(),
            dns: req.dns.unwrap_or_default(),
            mtu: req.mtu.unwrap_or(1420),
            backend: result.get("backend").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            active: result.get("status").and_then(|v| v.as_str()) == Some("ok"),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    );
    let _ = save_state(&state);

    info!("WireGuard tunnel '{}' configured", name);
    Ok(result)
}

/// Configure an IPsec tunnel.
pub fn configure_ipsec(req: IpsecConfigRequest) -> Result<serde_json::Value> {
    let name = req.name.unwrap_or_else(|| "ipsec0".to_string());

    let mut args = vec![
        "ipsec".to_string(), "config".to_string(),
        "--name".to_string(), name.clone(),
    ];

    if let Some(ref mode) = req.mode {
        args.push("--mode".to_string());
        args.push(mode.clone());
    }
    if let Some(ref proto) = req.proto {
        args.push("--proto".to_string());
        args.push(proto.clone());
    }
    args.push("--local-ip".to_string());
    args.push(req.local_ip.clone());
    args.push("--remote-ip".to_string());
    args.push(req.remote_ip.clone());

    if let Some(ref subnet) = req.local_subnet {
        args.push("--local-subnet".to_string());
        args.push(subnet.clone());
    }
    if let Some(ref subnet) = req.remote_subnet {
        args.push("--remote-subnet".to_string());
        args.push(subnet.clone());
    }
    if let Some(ref id) = req.local_id {
        args.push("--local-id".to_string());
        args.push(id.clone());
    }
    if let Some(ref id) = req.remote_id {
        args.push("--remote-id".to_string());
        args.push(id.clone());
    }
    if let Some(ref enc) = req.encryption {
        args.push("--encryption".to_string());
        args.push(enc.clone());
    }
    if let Some(ref integ) = req.integrity {
        args.push("--integrity".to_string());
        args.push(integ.clone());
    }
    if let Some(ref dh) = req.dh_group {
        args.push("--dh-group".to_string());
        args.push(dh.clone());
    }
    if let Some(ref ikelt) = req.ikelifetime {
        args.push("--ikelifetime".to_string());
        args.push(ikelt.clone());
    }
    if let Some(ref salt) = req.salifetime {
        args.push("--salifetime".to_string());
        args.push(salt.clone());
    }
    if let Some(ref psk) = req.pre_shared_key {
        args.push("--pre-shared-key".to_string());
        args.push(psk.clone());
    }

    let result = run_vpn_manager(&args)?;

    // Store in local state
    let mut state = load_state();
    state.ipsec.insert(
        name.clone(),
        IpsecConfig {
            name: name.clone(),
            mode: req.mode.unwrap_or_else(default_ipsec_mode),
            proto: req.proto.unwrap_or_else(default_ipsec_proto),
            local_ip: req.local_ip,
            remote_ip: req.remote_ip,
            local_subnet: req.local_subnet.unwrap_or_default(),
            remote_subnet: req.remote_subnet.unwrap_or_default(),
            local_id: req.local_id.unwrap_or_default(),
            remote_id: req.remote_id.unwrap_or_default(),
            encryption: req.encryption.unwrap_or_else(default_encryption),
            integrity: req.integrity.unwrap_or_else(default_integrity),
            dh_group: req.dh_group.unwrap_or_else(default_dh_group),
            ikelifetime: req.ikelifetime.unwrap_or_else(default_ikelifetime),
            salifetime: req.salifetime.unwrap_or_else(default_salifetime),
            pre_shared_key: req.pre_shared_key.unwrap_or_default(),
            backend: result.get("backend").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            active: result.get("status").and_then(|v| v.as_str()) == Some("ok"),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    );
    let _ = save_state(&state);

    info!("IPsec tunnel '{}' configured", name);
    Ok(result)
}

/// Configure OpenVPN.
pub fn configure_openvpn(req: OpenVpnConfigRequest) -> Result<serde_json::Value> {
    let name = req.name.unwrap_or_else(|| "ovpn0".to_string());

    let mut args = vec![
        "openvpn".to_string(), "config".to_string(),
        "--name".to_string(), name.clone(),
    ];

    if let Some(ref mode) = req.mode {
        args.push("--mode".to_string());
        args.push(mode.clone());
    }
    args.push("--remote".to_string());
    args.push(req.remote.clone());

    if let Some(port) = req.port {
        args.push("--port".to_string());
        args.push(port.to_string());
    }
    if let Some(ref proto) = req.proto {
        args.push("--proto".to_string());
        args.push(proto.clone());
    }
    if let Some(ref cert) = req.ca_cert {
        args.push("--ca-cert".to_string());
        args.push(cert.clone());
    }
    if let Some(ref cert) = req.client_cert {
        args.push("--client-cert".to_string());
        args.push(cert.clone());
    }
    if let Some(ref key) = req.client_key {
        args.push("--client-key".to_string());
        args.push(key.clone());
    }
    if let Some(ref auth) = req.tls_auth {
        args.push("--tls-auth".to_string());
        args.push(auth.clone());
    }
    if let Some(ref dev) = req.device {
        args.push("--device".to_string());
        args.push(dev.clone());
    }
    if let Some(ref cipher) = req.cipher {
        args.push("--cipher".to_string());
        args.push(cipher.clone());
    }
    if let Some(ref auth) = req.auth {
        args.push("--auth".to_string());
        args.push(auth.clone());
    }
    if let Some(true) = req.redirect_gateway {
        args.push("--redirect-gateway".to_string());
        args.push("true".to_string());
    }
    if let Some(ref dns) = req.dns_push {
        args.push("--dns-push".to_string());
        args.push(dns.clone());
    }

    let result = run_vpn_manager(&args)?;

    // Store in local state
    let mut state = load_state();
    state.openvpn.insert(
        name.clone(),
        OpenVpnConfig {
            name: name.clone(),
            mode: req.mode.unwrap_or_else(default_ovpn_mode),
            remote: req.remote,
            port: req.port.unwrap_or(1194),
            proto: req.proto.unwrap_or_else(default_ovpn_proto),
            ca_cert: req.ca_cert.unwrap_or_default(),
            client_cert: req.client_cert.unwrap_or_default(),
            client_key: req.client_key.unwrap_or_default(),
            tls_auth: req.tls_auth.unwrap_or_default(),
            device: req.device.unwrap_or_else(default_ovpn_device),
            cipher: req.cipher.unwrap_or_else(default_ovpn_cipher),
            auth: req.auth.unwrap_or_else(default_ovpn_auth),
            redirect_gateway: req.redirect_gateway.unwrap_or(false),
            dns_push: req.dns_push.unwrap_or_default(),
            backend: result.get("backend").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            active: result.get("status").and_then(|v| v.as_str()) == Some("ok"),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    );
    let _ = save_state(&state);

    info!("OpenVPN tunnel '{}' configured", name);
    Ok(result)
}

/// Bring down a tunnel by type and name.
pub fn bring_down(vpn_type: &str, name: &str) -> Result<serde_json::Value> {
    let args = match vpn_type {
        "wireguard" => vec!["wg".to_string(), "down".to_string(), "--name".to_string(), name.to_string()],
        "ipsec" => vec!["ipsec".to_string(), "down".to_string(), "--name".to_string(), name.to_string()],
        "openvpn" => vec!["openvpn".to_string(), "down".to_string(), "--name".to_string(), name.to_string()],
        _ => anyhow::bail!("Unknown VPN type: {}", vpn_type),
    };

    let result = run_vpn_manager(&args)?;

    // Update local state
    let mut state = load_state();
    match vpn_type {
        "wireguard" => { state.wireguard.remove(name); }
        "ipsec" => { state.ipsec.remove(name); }
        "openvpn" => { state.openvpn.remove(name); }
        _ => {}
    }
    let _ = save_state(&state);

    info!("VPN tunnel '{}' (type: {}) brought down", name, vpn_type);
    Ok(result)
}
