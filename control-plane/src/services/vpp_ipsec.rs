use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// VPP IPSec Manager
pub struct VppIpSecManager {
    script_path: String,
}

impl VppIpSecManager {
    pub fn new() -> Self {
        Self {
            script_path: "/root/VectorOS/vpp-tools/vpp_ipsec.py".to_string(),
        }
    }

    /// Create IKEv2 profile
    pub fn create_profile(&self, name: &str) -> Result<()> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("create")
            .arg("--name").arg(name)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create profile: {}", stderr);
        }

        Ok(())
    }

    /// Set authentication for IKEv2 profile
    pub fn set_auth(&self, name: &str, auth_type: &str, data: &str) -> Result<()> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("auth")
            .arg("--name").arg(name)
            .arg("--auth-type").arg(auth_type)
            .arg("--data").arg(data)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to set auth: {}", stderr);
        }

        Ok(())
    }

    /// Set identity for IKEv2 profile
    pub fn set_id(&self, name: &str, side: &str, id_type: &str, data: &str) -> Result<()> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("id")
            .arg("--name").arg(name)
            .arg("--side").arg(side)
            .arg("--id-type").arg(id_type)
            .arg("--data").arg(data)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to set id: {}", stderr);
        }

        Ok(())
    }

    /// Show IKEv2 Security Associations
    pub fn show_sa(&self) -> Result<Vec<Ikev2Sa>> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("sa")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show SAs: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        let mut sas = Vec::new();
        if let Some(arr) = result.get("sas").and_then(|v| v.as_array()) {
            for sa in arr {
                sas.push(Ikev2Sa {
                    ispi: sa.get("ispi").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    rspi: sa.get("rspi").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    state: sa.get("state").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    profile: sa.get("profile").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        Ok(sas)
    }

    /// Show IKEv2 profiles
    pub fn show_profiles(&self) -> Result<Vec<Ikev2Profile>> {
        let output = Command::new("python3")
            .arg(&self.script_path)
            .arg("profiles")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show profiles: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        let mut profiles = Vec::new();
        if let Some(arr) = result.get("profiles").and_then(|v| v.as_array()) {
            for profile in arr {
                profiles.push(Ikev2Profile {
                    name: profile.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    config: profile.get("config")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(profiles)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ikev2Sa {
    pub ispi: String,
    pub rspi: String,
    pub state: String,
    pub profile: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ikev2Profile {
    pub name: String,
    pub config: Vec<String>,
}
