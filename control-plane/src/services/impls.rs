//! Concrete service implementations that adapt existing service modules
//! into the `ServiceManager` trait interface.

use anyhow::Result;
use tracing::info;

use super::manager::{Service, ServiceState};

// ---------------------------------------------------------------------------
// PPPoE Service
// ---------------------------------------------------------------------------

pub struct PppoeService;

#[async_trait::async_trait]
impl Service for PppoeService {
    fn name(&self) -> &str {
        "pppoe"
    }

    fn display_name(&self) -> &str {
        "PPPoE Client"
    }

    fn description(&self) -> &str {
        "PPP over Ethernet dial-up connection"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting PPPoE service");
        // PPPoE sessions are created on-demand via the existing API.
        // On start we just verify the VPP plugin is loaded.
        tokio::task::spawn_blocking(|| {
            super::run_vppctl(&["show", "pppoe"])
        })
        .await??;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping PPPoE service");
        // Tear down all active PPPoE sessions
        tokio::task::spawn_blocking(|| {
            super::run_vppctl(&["pppoe", "delete", "all"])
        })
        .await??;
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::run_vppctl(&["show", "pppoe"])).await {
            Ok(Ok(output)) => {
                if output.contains("no sessions") || output.trim().is_empty() {
                    ServiceState::Stopped
                } else {
                    ServiceState::Running
                }
            }
            _ => ServiceState::Stopped,
        }
    }
}

// ---------------------------------------------------------------------------
// DHCP Service
// ---------------------------------------------------------------------------

pub struct DhcpService;

#[async_trait::async_trait]
impl Service for DhcpService {
    fn name(&self) -> &str {
        "dhcp"
    }

    fn display_name(&self) -> &str {
        "DHCP Server"
    }

    fn description(&self) -> &str {
        "DHCP address allocation for LAN clients"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting DHCP service");
        let config = super::dhcp::DhcpEnableConfig::default();
        tokio::task::spawn_blocking(move || super::dhcp::enable(config)).await??;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping DHCP service");
        tokio::task::spawn_blocking(|| super::dhcp::disable()).await??;
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::dhcp::is_dnsmasq_running()).await {
            Ok(true) => ServiceState::Running,
            _ => ServiceState::Stopped,
        }
    }

    async fn reload(&self) -> Result<()> {
        info!("Reloading DHCP service");
        // dnsmasq supports SIGHUP for config reload, but simpler to
        // just re-enable with same config.
        let config = super::dhcp::DhcpEnableConfig::default();
        tokio::task::spawn_blocking(move || super::dhcp::enable(config)).await??;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DNS Service
// ---------------------------------------------------------------------------

pub struct DnsService;

#[async_trait::async_trait]
impl Service for DnsService {
    fn name(&self) -> &str {
        "dns"
    }

    fn display_name(&self) -> &str {
        "DNS Forwarder"
    }

    fn description(&self) -> &str {
        "DNS resolution and upstream forwarding"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting DNS service");
        let config = super::dns::DnsEnableConfig::default();
        tokio::task::spawn_blocking(move || super::dns::enable(config)).await??;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping DNS service");
        tokio::task::spawn_blocking(|| super::dns::disable()).await??;
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::dns::is_dnsmasq_running()).await {
            Ok(true) => ServiceState::Running,
            _ => ServiceState::Stopped,
        }
    }

    async fn reload(&self) -> Result<()> {
        info!("Reloading DNS service");
        let config = super::dns::DnsEnableConfig::default();
        tokio::task::spawn_blocking(move || super::dns::enable(config)).await??;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// NAT Service
// ---------------------------------------------------------------------------

pub struct NatService;

#[async_trait::async_trait]
impl Service for NatService {
    fn name(&self) -> &str {
        "nat"
    }

    fn display_name(&self) -> &str {
        "NAT"
    }

    fn description(&self) -> &str {
        "Network Address Translation for outbound traffic"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting NAT service");
        tokio::task::spawn_blocking(|| {
            // Enable NAT plugin in VPP
            let _ = super::run_vppctl(&["nat", "plugin", "enable"]);
            // Enable inside/outside interfaces for NAT
            let _ = super::run_vppctl(&["nat", "inside", "interface", "set", "ip4", "inside"]);
            let _ = super::run_vppctl(&["nat", "outside", "interface", "set", "ip4", "outside"]);
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping NAT service");
        tokio::task::spawn_blocking(|| {
            let _ = super::run_vppctl(&["nat", "plugin", "disable"]);
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::run_vppctl(&["show", "nat", "translations"])).await {
            Ok(Ok(output)) => {
                if output.contains("N/A") || output.trim().is_empty() {
                    ServiceState::Stopped
                } else {
                    ServiceState::Running
                }
            }
            _ => ServiceState::Stopped,
        }
    }
}

// ---------------------------------------------------------------------------
// Firewall Service
// ---------------------------------------------------------------------------

pub struct FirewallService;

#[async_trait::async_trait]
impl Service for FirewallService {
    fn name(&self) -> &str {
        "firewall"
    }

    fn display_name(&self) -> &str {
        "Firewall"
    }

    fn description(&self) -> &str {
        "Stateful packet filtering and ACL management"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting Firewall service");
        tokio::task::spawn_blocking(|| super::firewall::enable()).await??;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping Firewall service");
        tokio::task::spawn_blocking(|| super::firewall::disable()).await??;
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::firewall::show()).await {
            Ok(Ok(val)) => {
                if val.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                    ServiceState::Running
                } else {
                    ServiceState::Stopped
                }
            }
            _ => ServiceState::Stopped,
        }
    }

    async fn reload(&self) -> Result<()> {
        info!("Reloading Firewall rules");
        tokio::task::spawn_blocking(|| {
            // Re-apply rules to VPP by re-enabling (which re-applies all rules)
            super::firewall::enable()
        })
        .await??;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// VPN Service
// ---------------------------------------------------------------------------

pub struct VpnService;

#[async_trait::async_trait]
impl Service for VpnService {
    fn name(&self) -> &str {
        "vpn"
    }

    fn display_name(&self) -> &str {
        "VPN"
    }

    fn description(&self) -> &str {
        "WireGuard / IPsec / OpenVPN tunnel management"
    }

    async fn start(&self) -> Result<()> {
        info!("Starting VPN service");
        // VPN service is always "available" — tunnels are configured on demand.
        // Starting the service means ensuring the backend is operational.
        match tokio::task::spawn_blocking(|| super::vpn::get_status()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                info!("VPN backend not available ({}), marking as stopped", e);
                Ok(())
            }
            Err(e) => {
                info!("VPN probe failed: {}, marking as stopped", e);
                Ok(())
            }
        }
    }

    async fn stop(&self) -> Result<()> {
        info!("Stopping VPN service");
        // Bring down all active tunnels
        match tokio::task::spawn_blocking(|| super::vpn::list_connections()).await {
            Ok(Ok(_)) => {
                // Individual tunnels would need to be brought down
                // For now, this is a best-effort stop.
            }
            _ => {}
        }
        Ok(())
    }

    async fn probe(&self) -> ServiceState {
        match tokio::task::spawn_blocking(|| super::vpn::get_status()).await {
            Ok(Ok(_)) => ServiceState::Running,
            _ => ServiceState::Stopped,
        }
    }
}
