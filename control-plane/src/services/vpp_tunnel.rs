use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// VPP Tunnel Manager
pub struct VppTunnelManager {
    script_path: String,
}

impl VppTunnelManager {
    pub fn new() -> Self {
        Self {
            script_path: "/root/VectorOS/vpp-tools/vpp_tunnel.py".to_string(),
        }
    }

    /// Create GRE tunnel
    pub fn create_gre(&self, src: &str, dst: &str) -> Result<String> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("gre-create")
            .arg("--src").arg(src)
            .arg("--dst").arg(dst)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create GRE tunnel: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("GRE error: {}", error);
        }

        Ok(result.get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("gre0")
            .to_string())
    }

    /// Create VXLAN tunnel
    pub fn create_vxlan(&self, src: &str, dst: &str, vni: u32) -> Result<String> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("vxlan-create")
            .arg("--src").arg(src)
            .arg("--dst").arg(dst)
            .arg("--vni").arg(vni.to_string())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create VXLAN tunnel: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("VXLAN error: {}", error);
        }

        Ok(result.get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("vxlan0")
            .to_string())
    }

    /// Show GRE tunnels
    pub fn show_gre(&self) -> Result<Vec<GreTunnel>> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("gre-show")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show GRE tunnels: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        let mut tunnels = Vec::new();
        if let Some(arr) = result.get("tunnels").and_then(|v| v.as_array()) {
            for t in arr {
                tunnels.push(GreTunnel {
                    name: t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    src: t.get("src").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    dst: t.get("dst").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(tunnels)
    }

    /// Show VXLAN tunnels
    pub fn show_vxlan(&self) -> Result<Vec<VxlanTunnel>> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("vxlan-show")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show VXLAN tunnels: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        let mut tunnels = Vec::new();
        if let Some(arr) = result.get("tunnels").and_then(|v| v.as_array()) {
            for t in arr {
                tunnels.push(VxlanTunnel {
                    name: t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    src: t.get("src").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    dst: t.get("dst").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    vni: t.get("vni").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                });
            }
        }

        Ok(tunnels)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GreTunnel {
    pub name: String,
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VxlanTunnel {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub vni: String,
}
