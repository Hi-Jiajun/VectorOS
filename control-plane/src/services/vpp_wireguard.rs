use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// VPP WireGuard Manager
pub struct VppWireGuardManager {
    script_path: String,
}

impl VppWireGuardManager {
    pub fn new() -> Self {
        Self {
            script_path: "/root/VectorOS/vpp-tools/vpp_wireguard.py".to_string(),
        }
    }

    /// Generate WireGuard keypair
    pub fn generate_keypair(&self) -> Result<(String, String)> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("genkey")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to generate keypair: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("Keypair error: {}", error);
        }

        let private_key = result.get("private_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let public_key = result.get("public_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok((private_key, public_key))
    }

    /// Create WireGuard interface
    pub fn create_interface(&self, listen_port: u16, private_key: &str, src_ip: Option<&str>) -> Result<String> {
        let mut args = vec![
            "create".to_string(),
            "--listen-port".to_string(),
            listen_port.to_string(),
            "--private-key".to_string(),
            private_key.to_string(),
        ];

        if let Some(ip) = src_ip {
            args.push("--src-ip".to_string());
            args.push(ip.to_string());
        }

        let output = Command::new("python3")
            .arg(&self.script_path)
            .args(&args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create WireGuard interface: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("Create interface error: {}", error);
        }

        Ok(result.get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("wg0")
            .to_string())
    }

    /// Add peer to WireGuard interface
    pub fn add_peer(
        &self,
        iface_name: &str,
        public_key: &str,
        endpoint: Option<&str>,
        port: Option<u16>,
        allowed_ips: &[String],
        preshared_key: Option<&str>,
    ) -> Result<()> {
        let mut args = vec![
            "peer-add".to_string(),
            "--interface".to_string(),
            iface_name.to_string(),
            "--public-key".to_string(),
            public_key.to_string(),
        ];

        if let Some(ep) = endpoint {
            args.push("--endpoint".to_string());
            args.push(ep.to_string());
        }

        if let Some(p) = port {
            args.push("--port".to_string());
            args.push(p.to_string());
        }

        if !allowed_ips.is_empty() {
            args.push("--allowed-ips".to_string());
            args.extend(allowed_ips.iter().cloned());
        }

        if let Some(psk) = preshared_key {
            args.push("--preshared-key".to_string());
            args.push(psk.to_string());
        }

        let output = Command::new("python3")
            .arg(&self.script_path)
            .args(&args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to add peer: {}", stderr);
        }

        Ok(())
    }

    /// Show WireGuard interfaces
    pub fn show_interfaces(&self) -> Result<Vec<WireGuardInterface>> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("show")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show interfaces: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        let mut interfaces = Vec::new();
        if let Some(arr) = result.get("interfaces").and_then(|v| v.as_array()) {
            for iface in arr {
                interfaces.push(WireGuardInterface {
                    name: iface.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    index: iface.get("index").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    state: iface.get("state").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(interfaces)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WireGuardInterface {
    pub name: String,
    pub index: String,
    pub state: String,
}
