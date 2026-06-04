//! Network diagnostics service
//!
//! Provides ping, traceroute, DNS lookup, and port scanning tools
//! similar to OpenWrt/ImmortalWrt luci-app-diag.
//! Runs system commands directly without Python dependency.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PingRequest {
    pub host: String,
    #[serde(default = "default_ping_count")]
    pub count: u32,
    pub size: Option<u32>,
}

fn default_ping_count() -> u32 {
    4
}

#[derive(Debug, Clone, Serialize)]
pub struct PingResult {
    pub host: String,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub packet_loss: String,
    pub rtt: RttStats,
    pub replies: Vec<PingReply>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RttStats {
    pub min: f64,
    pub avg: f64,
    pub max: f64,
    pub mdev: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PingReply {
    pub bytes: u32,
    pub source: String,
    pub icmp_seq: u32,
    pub ttl: u32,
    pub time_ms: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TracerouteRequest {
    pub host: String,
    #[serde(default = "default_max_hops")]
    pub max_hops: u32,
}

fn default_max_hops() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize)]
pub struct TracerouteResult {
    pub host: String,
    pub max_hops: u32,
    pub hop_count: usize,
    pub hops: Vec<TracerouteHop>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TracerouteHop {
    pub hop: u32,
    pub addresses: Vec<String>,
    pub times_ms: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsRequest {
    pub domain: String,
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsResult {
    pub domain: String,
    pub server: String,
    pub a_records: Vec<String>,
    pub aaaa_records: Vec<String>,
    pub soa_record: Option<String>,
    pub mx_records: Vec<String>,
    pub ns_records: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PortScanRequest {
    pub host: String,
    pub ports: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortScanResult {
    pub host: String,
    pub target_ip: String,
    pub ports_scanned: u32,
    pub open_count: u32,
    pub closed_count: u32,
    pub open_ports: Vec<OpenPort>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenPort {
    pub port: u32,
    pub state: String,
    pub service: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    let mut year = 1970i64;
    let mut day_of_year = days as i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if day_of_year < days_in_year {
            break;
        }
        day_of_year -= days_in_year;
        year += 1;
    }
    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    let mut day = day_of_year as u32 + 1;
    for (i, &md) in month_days.iter().enumerate() {
        if day <= md {
            month = (i + 1) as u32;
            break;
        }
        day -= md;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ---------------------------------------------------------------------------
// Ping
// ---------------------------------------------------------------------------

pub fn ping(req: PingRequest) -> Result<serde_json::Value> {
    let mut cmd = Command::new("ping");
    cmd.arg("-c").arg(req.count.to_string());
    if let Some(size) = req.size {
        cmd.arg("-s").arg(size.to_string());
    }
    cmd.arg(&req.host);

    let output = cmd
        .output()
        .context("Failed to execute ping command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let lines: Vec<&str> = stdout.lines().collect();

    let mut packets_sent = 0u32;
    let mut packets_received = 0u32;
    let mut packet_loss = "100%".to_string();
    let mut rtt_min = 0.0f64;
    let mut rtt_avg = 0.0f64;
    let mut rtt_max = 0.0f64;
    let mut rtt_mdev = 0.0f64;
    let mut replies: Vec<PingReply> = Vec::new();

    for line in &lines {
        // Parse individual reply lines
        // "64 bytes from 8.8.8.8: icmp_seq=1 ttl=116 time=12.3 ms"
        if let Some(caps) = parse_ping_reply(line) {
            replies.push(PingReply {
                bytes: caps.0,
                source: caps.1,
                icmp_seq: caps.2,
                ttl: caps.3,
                time_ms: caps.4,
            });
        }

        // Parse statistics line
        // "3 packets transmitted, 3 received, 0% packet loss"
        if line.contains("transmitted") && line.contains("received") {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                if let Some(n) = extract_number(parts[0]) {
                    packets_sent = n;
                }
                if let Some(n) = extract_number(parts[1]) {
                    packets_received = n;
                }
                // Extract loss percentage
                if let Some(pct) = parts[2].split('%').next() {
                    packet_loss = format!("{}%", pct.trim());
                }
            }
        }

        // Parse RTT line
        // "rtt min/avg/max/mdev = 12.345/13.456/14.567/1.234 ms"
        if line.contains("rtt min/avg/max/mdev") {
            if let Some(eq_pos) = line.find('=') {
                let values_str = line[eq_pos + 1..].trim().trim_end_matches("ms").trim();
                let values: Vec<&str> = values_str.split('/').collect();
                if values.len() >= 4 {
                    rtt_min = values[0].trim().parse().unwrap_or(0.0);
                    rtt_avg = values[1].trim().parse().unwrap_or(0.0);
                    rtt_max = values[2].trim().parse().unwrap_or(0.0);
                    rtt_mdev = values[3].trim().parse().unwrap_or(0.0);
                }
            }
        }
    }

    Ok(serde_json::json!({
        "status": "ok",
        "host": req.host,
        "packets_sent": packets_sent,
        "packets_received": packets_received,
        "packet_loss": packet_loss,
        "rtt": {
            "min": rtt_min,
            "avg": rtt_avg,
            "max": rtt_max,
            "mdev": rtt_mdev,
        },
        "replies": replies,
    }))
}

/// Parse a ping reply line, returning (bytes, source_ip, icmp_seq, ttl, time_ms).
fn parse_ping_reply(line: &str) -> Option<(u32, String, u32, u32, f64)> {
    let line = line.trim();
    if !line.starts_with("64 bytes from") && !line.starts_with("bytes from") {
        return None;
    }

    // Extract bytes
    let bytes = line.split_whitespace().next()?.parse::<u32>().ok()?;

    // Extract source IP
    let from_start = line.find("from ")? + 5;
    let from_end = line[from_start..].find(':')? + from_start;
    let source = line[from_start..from_end].to_string();

    // Extract icmp_seq
    let icmp_seq = extract_field(line, "icmp_seq=")?.parse::<u32>().ok()?;

    // Extract ttl
    let ttl = extract_field(line, "ttl=")?.parse::<u32>().ok()?;

    // Extract time
    let time_str = extract_field(line, "time=")?;
    let time_ms = time_str.parse::<f64>().ok()?;

    Some((bytes, source, icmp_seq, ttl, time_ms))
}

fn extract_field<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let pos = line.find(prefix)?;
    let start = pos + prefix.len();
    let rest = &line[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
    Some(&rest[..end])
}

fn extract_number(s: &str) -> Option<u32> {
    s.split_whitespace()
        .find_map(|w| w.parse::<u32>().ok())
}

// ---------------------------------------------------------------------------
// Traceroute
// ---------------------------------------------------------------------------

pub fn traceroute(req: TracerouteRequest) -> Result<serde_json::Value> {
    let output = Command::new("traceroute")
        .arg("-n")
        .arg("-m")
        .arg(req.max_hops.to_string())
        .arg(&req.host)
        .output()
        .context("Failed to execute traceroute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut hops: Vec<TracerouteHop> = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("traceroute") {
            continue;
        }

        // Parse: " 1  192.168.1.1  1.234 ms  2.345 ms  3.456 ms"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        if let Ok(hop_num) = parts[0].parse::<u32>() {
            let rest = &line[parts[0].len()..];

            // Collect IP addresses (non-numeric parts that look like IPs)
            let mut addresses: Vec<String> = Vec::new();
            let mut times_ms: Vec<f64> = Vec::new();

            for part in &parts[1..] {
                if part.contains('.') && !part.ends_with("ms") {
                    // Might be an IP address
                    let cleaned = part.trim_matches('*');
                    if !cleaned.is_empty() && cleaned.parse::<std::net::Ipv4Addr>().is_ok() {
                        addresses.push(cleaned.to_string());
                    }
                } else if part.ends_with("ms") {
                    if let Ok(t) = part.trim_end_matches("ms").parse::<f64>() {
                        times_ms.push(t);
                    }
                } else if *part == "*" {
                    // Timeout probe
                }
            }

            let is_timeout = addresses.is_empty() && times_ms.is_empty();

            hops.push(TracerouteHop {
                hop: hop_num,
                addresses,
                times_ms,
                timeout: if is_timeout { Some(true) } else { None },
            });
        }
    }

    Ok(serde_json::json!({
        "status": "ok",
        "host": req.host,
        "max_hops": req.max_hops,
        "hop_count": hops.len(),
        "hops": hops,
    }))
}

// ---------------------------------------------------------------------------
// DNS Lookup
// ---------------------------------------------------------------------------

pub fn dns_lookup(req: DnsRequest) -> Result<serde_json::Value> {
    let server_arg = req.server.as_deref().unwrap_or("");

    // Dig A records
    let a_records = dig_records(&req.domain, "A", server_arg);
    // Dig AAAA records
    let aaaa_records = dig_records(&req.domain, "AAAA", server_arg);
    // Dig MX records
    let mx_records = dig_records(&req.domain, "MX", server_arg);
    // Dig NS records
    let ns_records = dig_records(&req.domain, "NS", server_arg);

    // Try SOA
    let soa_record = {
        let output = dig_query(&req.domain, "SOA", server_arg);
        let lines: Vec<&str> = output
            .lines()
            .filter(|l| !l.starts_with(';') && !l.trim().is_empty())
            .collect();
        if lines.is_empty() {
            None
        } else {
            Some(lines[0].trim().to_string())
        }
    };

    Ok(serde_json::json!({
        "status": "ok",
        "domain": req.domain,
        "server": req.server.unwrap_or_else(|| "system default".to_string()),
        "a_records": a_records,
        "aaaa_records": aaaa_records,
        "soa_record": soa_record,
        "mx_records": mx_records,
        "ns_records": ns_records,
    }))
}

fn dig_records(domain: &str, record_type: &str, server: &str) -> Vec<String> {
    let output = dig_query(domain, record_type, server);
    output
        .lines()
        .filter(|l| !l.starts_with(';') && !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn dig_query(domain: &str, record_type: &str, server: &str) -> String {
    let mut cmd = Command::new("dig");
    if !server.is_empty() {
        cmd.arg(format!("@{}", server));
    }
    cmd.arg("+short").arg(record_type).arg(domain);

    match cmd.output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Port Scan
// ---------------------------------------------------------------------------

pub fn port_scan(req: PortScanRequest) -> Result<serde_json::Value> {
    use std::net::TcpStream;
    use std::time::Duration;

    // Resolve hostname
    use std::net::ToSocketAddrs;
    let target_ip = format!("{}:0", req.host)
        .to_socket_addrs()
        .context("Failed to resolve hostname")?
        .next()
        .context("No addresses found for hostname")?
        .ip()
        .to_string();

    // Parse port range
    let ports = parse_port_range(&req.ports).context("Invalid port specification")?;
    if ports.len() > 1024 {
        return Ok(serde_json::json!({
            "error": "Maximum 1024 ports per scan",
            "port_count": ports.len(),
        }));
    }

    let mut open_ports: Vec<OpenPort> = Vec::new();
    let mut closed_count = 0u32;

    for &port in &ports {
        let addr = format!("{}:{}", target_ip, port);
        match TcpStream::connect_timeout(
            &addr.parse().context("Invalid address")?,
            Duration::from_secs(2),
        ) {
            Ok(_) => {
                let service = common_service(port);
                open_ports.push(OpenPort {
                    port,
                    state: "open".to_string(),
                    service: service.to_string(),
                });
            }
            Err(_) => {
                closed_count += 1;
            }
        }
    }

    Ok(serde_json::json!({
        "status": "ok",
        "host": req.host,
        "target_ip": target_ip,
        "ports_scanned": ports.len(),
        "open_count": open_ports.len(),
        "closed_count": closed_count,
        "open_ports": open_ports,
    }))
}

fn parse_port_range(spec: &str) -> Result<Vec<u32>> {
    let mut ports = std::collections::HashSet::new();
    for part in spec.split(',') {
        let part = part.trim();
        if let Some((start_str, end_str)) = part.split_once('-') {
            let start: u32 = start_str.trim().parse().context("Invalid port")?;
            let end: u32 = end_str.trim().parse().context("Invalid port")?;
            if start > end {
                anyhow::bail!("Port range start > end: {} > {}", start, end);
            }
            for p in start..=end {
                ports.insert(p);
            }
        } else {
            let p: u32 = part.parse().context("Invalid port")?;
            ports.insert(p);
        }
    }
    let mut result: Vec<u32> = ports.into_iter().collect();
    result.sort();
    Ok(result)
}

fn common_service(port: u32) -> &'static str {
    match port {
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        80 => "HTTP",
        110 => "POP3",
        143 => "IMAP",
        443 => "HTTPS",
        993 => "IMAPS",
        995 => "POP3S",
        3306 => "MySQL",
        5432 => "PostgreSQL",
        6379 => "Redis",
        8080 => "HTTP-Alt",
        8443 => "HTTPS-Alt",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Summary (combined diagnostics status)
// ---------------------------------------------------------------------------

pub fn get_status() -> Result<serde_json::Value> {
    // Check which diagnostic tools are available
    let ping_available = Command::new("which")
        .arg("ping")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let traceroute_available = Command::new("which")
        .arg("traceroute")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let dig_available = Command::new("which")
        .arg("dig")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok(serde_json::json!({
        "status": "ok",
        "timestamp": chrono_now(),
        "tools": {
            "ping": ping_available,
            "traceroute": traceroute_available,
            "dns_lookup": dig_available,
            "port_scan": true,
        },
    }))
}
