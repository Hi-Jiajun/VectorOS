use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error};

/// DHCP lease
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpLease {
    pub mac: String,
    pub ip: Ipv4Addr,
    pub hostname: String,
    pub lease_time: u32,
    pub created_at: u64,
    pub expires_at: u64,
}

/// DHCP server configuration
#[derive(Debug, Clone)]
pub struct DhcpConfig {
    pub interface: String,
    pub gateway: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub start_ip: Ipv4Addr,
    pub end_ip: Ipv4Addr,
    pub dns_servers: Vec<Ipv4Addr>,
    pub lease_time: u32,
}

/// Simple DHCP server for LAN clients
pub struct DhcpServer {
    config: DhcpConfig,
    leases: Arc<RwLock<HashMap<String, DhcpLease>>>,
    allocated_ips: Arc<RwLock<Vec<Ipv4Addr>>>,
    running: Arc<AtomicBool>,
}

impl DhcpServer {
    pub fn new(config: DhcpConfig) -> Self {
        Self {
            config,
            leases: Arc::new(RwLock::new(HashMap::new())),
            allocated_ips: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the DHCP server
    pub fn start(&self) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:67")?;
        socket.set_broadcast(true)?;
        socket.set_read_timeout(Some(Duration::from_secs(1)))?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let leases = self.leases.clone();
        let allocated = self.allocated_ips.clone();
        let config = self.config.clone();

        info!("DHCP server listening on port 67");

        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buf) {
                    Ok((len, addr)) => {
                        let data = buf[..len].to_vec();
                        let config_clone = config.clone();
                        let leases_clone = leases.clone();
                        let allocated_clone = allocated.clone();
                        let socket_clone = socket.try_clone().unwrap();

                        thread::spawn(move || {
                            if let Err(e) = Self::handle_dhcp(
                                &socket_clone,
                                &data,
                                &config_clone,
                                &leases_clone,
                                &allocated_clone,
                            ) {
                                warn!("DHCP error: {}", e);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        if running.load(Ordering::SeqCst) {
                            error!("DHCP server error: {}", e);
                        }
                    }
                }
            }
            info!("DHCP server stopped");
        });

        Ok(())
    }

    /// Stop the DHCP server
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Handle a DHCP message
    fn handle_dhcp(
        socket: &UdpSocket,
        data: &[u8],
        config: &DhcpConfig,
        leases: &Arc<RwLock<HashMap<String, DhcpLease>>>,
        allocated: &Arc<RwLock<Vec<Ipv4Addr>>>,
    ) -> Result<()> {
        if data.len() < 240 {
            return Ok(());
        }

        // Get message type from options
        let dhcp_type = Self::get_dhcp_option(data, 53);
        let dhcp_type = match dhcp_type {
            Some(t) if !t.is_empty() => t,
            _ => return Ok(()),
        };

        let msg_type = dhcp_type[0];
        let mac = format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            data[28], data[29], data[30], data[31], data[32], data[33]
        );

        match msg_type {
            1 => Self::handle_discover(socket, data, &mac, config, leases, allocated),
            3 => Self::handle_request(socket, data, &mac, config, leases, allocated),
            7 => {
                // DHCPRELEASE
                if let Ok(mut leases) = leases.write() {
                    if let Some(lease) = leases.remove(&mac) {
                        if let Ok(mut alloc) = allocated.write() {
                            alloc.retain(|ip| *ip != lease.ip);
                        }
                        info!("Released IP {} for MAC {}", lease.ip, mac);
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Handle DHCPDISCOVER
    fn handle_discover(
        socket: &UdpSocket,
        data: &[u8],
        mac: &str,
        config: &DhcpConfig,
        leases: &Arc<RwLock<HashMap<String, DhcpLease>>>,
        allocated: &Arc<RwLock<Vec<Ipv4Addr>>>,
    ) -> Result<()> {
        let ip = Self::allocate_ip(mac, config, leases, allocated)?;
        let response = Self::build_response(data, 2, ip, config); // DHCPOFFER
        socket.send_to(&response, "255.255.255.255:68")?;
        info!("DHCPOFFER: {} -> {}", mac, ip);
        Ok(())
    }

    /// Handle DHCPREQUEST
    fn handle_request(
        socket: &UdpSocket,
        data: &[u8],
        mac: &str,
        config: &DhcpConfig,
        leases: &Arc<RwLock<HashMap<String, DhcpLease>>>,
        allocated: &Arc<RwLock<Vec<Ipv4Addr>>>,
    ) -> Result<()> {
        let ip = Self::allocate_ip(mac, config, leases, allocated)?;
        let response = Self::build_response(data, 5, ip, config); // DHCPACK
        socket.send_to(&response, "255.255.255.255:68")?;
        info!("DHCPACK: {} -> {}", mac, ip);
        Ok(())
    }

    /// Allocate an IP address for a MAC
    fn allocate_ip(
        mac: &str,
        config: &DhcpConfig,
        leases: &Arc<RwLock<HashMap<String, DhcpLease>>>,
        allocated: &Arc<RwLock<Vec<Ipv4Addr>>>,
    ) -> Result<Ipv4Addr> {
        // Check existing lease
        if let Ok(leases) = leases.read() {
            if let Some(lease) = leases.get(mac) {
                if lease.expires_at > Self::now() {
                    return Ok(lease.ip);
                }
            }
        }

        // Find available IP
        let start = u32::from(config.start_ip);
        let end = u32::from(config.end_ip);

        let allocated_ips = allocated.read().unwrap().clone();

        for ip_int in start..=end {
            let ip = Ipv4Addr::from(ip_int);
            if !allocated_ips.contains(&ip) {
                // Allocate this IP
                if let Ok(mut alloc) = allocated.write() {
                    alloc.push(ip);
                }

                let now = Self::now();
                let lease = DhcpLease {
                    mac: mac.to_string(),
                    ip,
                    hostname: String::new(),
                    lease_time: config.lease_time,
                    created_at: now,
                    expires_at: now + config.lease_time as u64,
                };

                if let Ok(mut leases) = leases.write() {
                    leases.insert(mac.to_string(), lease);
                }

                return Ok(ip);
            }
        }

        anyhow::bail!("No available IP addresses")
    }

    /// Build a DHCP response
    fn build_response(request: &[u8], msg_type: u8, ip: Ipv4Addr, config: &DhcpConfig) -> Vec<u8> {
        let mut response = vec![0u8; 240];

        // BOOTREPLY
        response[0] = 2;
        // Hardware type: Ethernet
        response[1] = 1;
        // Hardware address length
        response[2] = 6;
        // Transaction ID (copy from request)
        response[4..8].copy_from_slice(&request[4..8]);
        // Flags (broadcast)
        response[10..12].copy_from_slice(&[0x80, 0x00]);
        // Your IP address
        response[16..20].copy_from_slice(&ip.octets());
        // Server IP address
        response[20..24].copy_from_slice(&config.gateway.octets());
        // Client MAC (copy from request)
        response[28..34].copy_from_slice(&request[28..34]);

        // Build options
        let mut options = Vec::new();

        // DHCP Message Type
        options.extend_from_slice(&[53, 1, msg_type]);

        // Server Identifier
        options.extend_from_slice(&[54, 4]);
        options.extend_from_slice(&config.gateway.octets());

        // IP Address Lease Time
        options.extend_from_slice(&[51, 4]);
        options.extend_from_slice(&config.lease_time.to_be_bytes());

        // Subnet Mask
        options.extend_from_slice(&[1, 4]);
        options.extend_from_slice(&config.subnet_mask.octets());

        // Router
        options.extend_from_slice(&[3, 4]);
        options.extend_from_slice(&config.gateway.octets());

        // DNS Servers
        let dns_count = config.dns_servers.len() as u8;
        options.extend_from_slice(&[6, dns_count * 4]);
        for dns in &config.dns_servers {
            options.extend_from_slice(&dns.octets());
        }

        // End option
        options.push(255);

        let mut result = response;
        result.extend_from_slice(&options);
        result
    }

    /// Get a DHCP option from the options area
    fn get_dhcp_option(data: &[u8], option_code: u8) -> Option<Vec<u8>> {
        let mut offset = 240; // Skip fixed header
        while offset < data.len() {
            if offset + 2 > data.len() {
                break;
            }
            let code = data[offset];
            if code == 255 {
                break;
            }
            if code == 0 {
                offset += 1;
                continue;
            }
            let length = data[offset + 1] as usize;
            if offset + 2 + length > data.len() {
                break;
            }
            if code == option_code {
                return Some(data[offset + 2..offset + 2 + length].to_vec());
            }
            offset += 2 + length;
        }
        None
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
