//! IPv6 management service
//!
//! Manages IPv6 addresses, NDP (Neighbor Discovery Protocol), and routes
//! via VPP CLI commands (vppctl). No Python dependency.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

/// IPv6 interface with its addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipv6Interface {
    pub name: String,
    pub ipv6_addresses: Vec<String>,
}

/// NDP neighbor entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdpNeighbor {
    pub ipv6: String,
    pub mac: String,
    pub interface: String,
    pub flags: String,
}

/// IPv6 route entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipv6Route {
    pub destination: String,
    pub next_hop: String,
    pub details: String,
}

/// Execute a vppctl command and return the output.
fn run_vppctl(args: &[&str]) -> Result<String> {
    let output = Command::new("vppctl")
        .args(args)
        .output()
        .context("Failed to execute vppctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("vppctl failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Set an IPv6 address on a VPP interface.
///
/// The address must include a prefix length (e.g. "2001:db8::1/64").
pub fn set_address(interface: &str, address: &str) -> Result<serde_json::Value> {
    if !address.contains('/') {
        anyhow::bail!("IPv6 address must include prefix length (e.g. 2001:db8::1/64)");
    }

    run_vppctl(&["set", "interface", "ip6", "address", interface, address])?;
    info!("IPv6 address {} set on {}", address, interface);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("IPv6 address {} set on {}", address, interface)
    }))
}

/// Remove an IPv6 address from a VPP interface.
pub fn del_address(interface: &str, address: &str) -> Result<serde_json::Value> {
    if !address.contains('/') {
        anyhow::bail!("IPv6 address must include prefix length");
    }

    // Try set command first (some VPP versions), fall back to del command
    let result = run_vppctl(&["set", "interface", "ip6", "address", interface, address]);
    if result.is_err() {
        run_vppctl(&["del", "interface", "ip6", "address", interface, address])?;
    }

    info!("IPv6 address {} removed from {}", address, interface);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("IPv6 address {} removed from {}", address, interface)
    }))
}

/// Show IPv6 addresses on all VPP interfaces.
pub fn show() -> Result<serde_json::Value> {
    let output = run_vppctl(&["show", "interface", "addr"])?;
    let mut interfaces: Vec<Ipv6Interface> = Vec::new();
    let mut current_iface: Option<Ipv6Interface> = None;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Interface header lines don't start with spaces and contain state info
        if !line.starts_with(' ') && (line.to_lowercase().contains("up") || line.to_lowercase().contains("down")) {
            // Save previous interface
            if let Some(iface) = current_iface.take() {
                interfaces.push(iface);
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(name) = parts.first() {
                current_iface = Some(Ipv6Interface {
                    name: name.to_string(),
                    ipv6_addresses: Vec::new(),
                });
            }
        } else if let Some(ref mut iface) = current_iface {
            // Address line - check if it contains an IPv6 address
            if line.contains(':') && line.contains('/') {
                let addr = line.split_whitespace().next().unwrap_or(line);
                if addr.contains(':') {
                    iface.ipv6_addresses.push(addr.to_string());
                }
            }
        }
    }

    // Don't forget the last interface
    if let Some(iface) = current_iface {
        interfaces.push(iface);
    }

    Ok(serde_json::json!({
        "interfaces": interfaces,
    }))
}

/// Show the IPv6 neighbor discovery (NDP) table.
pub fn show_ndp() -> Result<serde_json::Value> {
    let output = run_vppctl(&["show", "ip6", "neighbors"])?;
    let mut neighbors = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.contains("IP6")
            || line.contains("---")
            || line.contains("IPv6")
        {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0].contains(':') {
            neighbors.push(NdpNeighbor {
                ipv6: parts[0].to_string(),
                mac: parts[1].to_string(),
                interface: parts[2].to_string(),
                flags: parts[3].to_string(),
            });
        }
    }

    Ok(serde_json::json!({
        "neighbors": neighbors,
    }))
}

/// Enable IPv6 neighbor discovery on a VPP interface.
pub fn enable_ndp(interface: &str) -> Result<serde_json::Value> {
    let result = run_vppctl(&["set", "interface", "ip6", "enable", interface]);

    if let Err(ref e) = result {
        // Ignore "already enabled" errors
        if !e.to_string().contains("already") {
            return Err(anyhow::anyhow!("Failed to enable IPv6 on {}: {}", interface, e));
        }
    }

    info!("IPv6 NDP enabled on {}", interface);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("IPv6 NDP enabled on {}", interface)
    }))
}

/// Show the IPv6 routing table (FIB).
pub fn show_routes() -> Result<serde_json::Value> {
    let output = run_vppctl(&["show", "ip6", "fib"])?;
    let mut routes = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains("IP6") || line.contains("---") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].contains(':') {
            routes.push(Ipv6Route {
                destination: parts[0].to_string(),
                next_hop: parts[1].to_string(),
                details: parts[2..].join(" "),
            });
        }
    }

    Ok(serde_json::json!({
        "routes": routes,
    }))
}

/// Add a static IPv6 route.
pub fn add_route(destination: &str, next_hop: &str) -> Result<serde_json::Value> {
    run_vppctl(&["ip", "route", "add", destination, "via", next_hop])?;
    info!("IPv6 route {} via {} added", destination, next_hop);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Route {} via {} added", destination, next_hop)
    }))
}

/// Delete a static IPv6 route.
pub fn del_route(destination: &str, next_hop: &str) -> Result<serde_json::Value> {
    run_vppctl(&["ip", "route", "del", destination, "via", next_hop])?;
    info!("IPv6 route {} via {} removed", destination, next_hop);

    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Route {} via {} removed", destination, next_hop)
    }))
}
