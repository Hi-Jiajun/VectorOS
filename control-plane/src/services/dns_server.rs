use anyhow::Result;
use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tracing::{info, warn, error};

/// Simple DNS server for LAN clients
/// Forwards queries to upstream DNS servers and caches responses
pub struct DnsServer {
    listen_addr: String,
    listen_port: u16,
    upstream_servers: Vec<String>,
    running: Arc<AtomicBool>,
}

impl DnsServer {
    pub fn new(listen_addr: &str, listen_port: u16, upstream_servers: Vec<String>) -> Self {
        Self {
            listen_addr: listen_addr.to_string(),
            listen_port,
            upstream_servers,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the DNS server
    pub fn start(&self) -> Result<()> {
        let socket = UdpSocket::bind(format!("{}:{}", self.listen_addr, self.listen_port))?;
        socket.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let upstream = self.upstream_servers.clone();

        info!("DNS server listening on {}:{}", self.listen_addr, self.listen_port);

        thread::spawn(move || {
            let mut buf = [0u8; 512];
            while running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buf) {
                    Ok((len, addr)) => {
                        let data = buf[..len].to_vec();
                        let upstream_clone = upstream.clone();
                        let socket_clone = socket.try_clone().unwrap();

                        thread::spawn(move || {
                            if let Err(e) = Self::handle_query(&socket_clone, &data, addr, &upstream_clone) {
                                warn!("DNS query error: {}", e);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        if running.load(Ordering::SeqCst) {
                            error!("DNS server error: {}", e);
                        }
                    }
                }
            }
            info!("DNS server stopped");
        });

        Ok(())
    }

    /// Stop the DNS server
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Handle a DNS query by forwarding to upstream
    fn handle_query(
        socket: &UdpSocket,
        query: &[u8],
        addr: std::net::SocketAddr,
        upstream_servers: &[String],
    ) -> Result<()> {
        // Forward to upstream DNS servers
        for server in upstream_servers {
            let upstream_addr = format!("{}:53", server);
            let upstream_socket = UdpSocket::bind("0.0.0.0:0")?;
            upstream_socket.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;

            if upstream_socket.send_to(query, &upstream_addr).is_ok() {
                let mut response = [0u8; 512];
                if let Ok(len) = upstream_socket.recv(&mut response) {
                    // Send response back to client
                    socket.send_to(&response[..len], addr)?;
                    return Ok(());
                }
            }
        }

        // If all upstreams failed, send SERVFAIL
        let response = Self::build_servfail(query);
        socket.send_to(&response, addr)?;
        Ok(())
    }

    /// Build a SERVFAIL response
    fn build_servfail(query: &[u8]) -> Vec<u8> {
        let mut response = query.to_vec();

        // Set QR bit (response)
        if response.len() > 2 {
            response[2] |= 0x80;
        }

        // Set RCODE to 2 (SERVFAIL)
        if response.len() > 3 {
            response[3] = (response[3] & 0xF0) | 0x02;
        }

        response
    }
}
