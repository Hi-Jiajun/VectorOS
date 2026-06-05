use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// VPP ACL Rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    pub index: u32,
    pub action: String,  // permit, deny
    pub src: String,     // IP/CIDR
    pub dst: String,     // IP/CIDR
    pub proto: u8,       // 0=any, 6=tcp, 17=udp
    pub sport: String,   // port range "0-65535"
    pub dport: String,   // port range "0-65535"
    pub tag: String,
}

/// VPP ACL Manager
pub struct VppAclManager {
    acl_script: String,
}

impl VppAclManager {
    pub fn new() -> Self {
        Self {
            acl_script: "/root/VectorOS/vpp-tools/vpp_acl.py".to_string(),
        }
    }

    /// Add ACL rule
    pub fn add_rule(&self, rule: &AclRule) -> Result<u32> {
        let output = Command::new("python3")
            .arg(&self.acl_script)
            .arg("add")
            .arg("--action-type").arg(&rule.action)
            .arg("--src").arg(&rule.src)
            .arg("--dst").arg(&rule.dst)
            .arg("--proto").arg(rule.proto.to_string())
            .arg("--sport").arg(&rule.sport)
            .arg("--dport").arg(&rule.dport)
            .arg("--tag").arg(&rule.tag)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to add ACL: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        if let Some(error) = result.get("error") {
            anyhow::bail!("ACL error: {}", error);
        }

        Ok(result.get("acl_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32)
    }

    /// Delete ACL rule
    pub fn delete_rule(&self, index: u32) -> Result<()> {
        let output = Command::new("python3")
            .arg(&self.acl_script)
            .arg("delete")
            .arg("--index").arg(index.to_string())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to delete ACL: {}", stderr);
        }

        Ok(())
    }

    /// Show all ACL rules
    pub fn show_rules(&self) -> Result<Vec<AclRule>> {
        let output = Command::new("python3")
            .arg(&self.acl_script)
            .arg("show")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to show ACLs: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)?;

        // Parse rules from VPP output
        let mut rules = Vec::new();
        if let Some(rules_array) = result.get("rules").and_then(|v| v.as_array()) {
            for rule_value in rules_array {
                if let Some(rule_str) = rule_value.get("rules").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.as_str()) {
                    // Parse rule string: "ipv4 permit src 0.0.0.0/0 dst 0.0.0.0/0 ..."
                    let parts: Vec<&str> = rule_str.split_whitespace().collect();
                    if parts.len() >= 6 {
                        rules.push(AclRule {
                            index: rule_value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            action: parts[1].to_string(),
                            src: parts[3].to_string(),
                            dst: parts[5].to_string(),
                            proto: 0,
                            sport: "0-65535".to_string(),
                            dport: "0-65535".to_string(),
                            tag: rule_value.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        });
                    }
                }
            }
        }

        Ok(rules)
    }

    /// Apply ACL to interface
    pub fn apply_to_interface(&self, interface: &str, acl_index: u32, input: bool) -> Result<()> {
        let direction = if input { "input" } else { "output" };
        let output = Command::new("python3")
            .arg(&self.acl_script)
            .arg("apply")
            .arg("--interface").arg(interface)
            .arg("--index").arg(acl_index.to_string())
            .arg("--direction").arg(direction)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to apply ACL: {}", stderr);
        }

        Ok(())
    }
}
