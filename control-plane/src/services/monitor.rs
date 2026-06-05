//! System Monitoring Service
//!
//! Collects system metrics every 5 seconds, stores them in SQLite,
//! calculates rates and trends, and generates alerts.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn, error};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Top-level metrics snapshot collected by the monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: String,
    pub cpu_percent: f64,
    pub cpu_count: u32,
    pub cpu_cores: Vec<CoreMetric>,
    pub memory: MemoryMetric,
    pub disk_usage: Vec<DiskMetric>,
    pub disk_io: DiskIoMetric,
    pub network: Vec<NetworkMetric>,
    pub vpp: VppMetric,
    pub processes: Vec<ProcessMetric>,
    pub temperatures: Vec<TemperatureMetric>,
    pub load_average: LoadAverage,
    pub uptime: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMetric {
    pub core: u32,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetric {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub buffers: u64,
    pub cached: u64,
    pub available: u64,
    pub percent: f64,
    #[serde(default)]
    pub swap_total: u64,
    #[serde(default)]
    pub swap_used: u64,
    #[serde(default)]
    pub swap_free: u64,
    #[serde(default)]
    pub swap_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetric {
    pub device: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub percent: f64,
    pub mountpoint: String,
    #[serde(default)]
    pub health: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoMetric {
    pub total_read_bytes_per_sec: u64,
    pub total_write_bytes_per_sec: u64,
    pub devices: Vec<DiskIoDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoDevice {
    pub name: String,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetric {
    pub name: String,
    pub state: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_drops: u64,
    pub tx_drops: u64,
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub rx_pps: u64,
    pub tx_pps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppMetric {
    pub available: bool,
    pub version: String,
    pub nat_sessions: u32,
    pub pppoe_active: u32,
    pub pppoe_discovery: u32,
    pub pppoe_total: u32,
    pub memory_total_mb: f64,
    pub memory_used_mb: f64,
    pub memory_percent: f64,
    pub errors_total: u64,
    #[serde(default)]
    pub worker_threads: u32,
    #[serde(default)]
    pub packet_rate_rx: u64,
    #[serde(default)]
    pub packet_rate_tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetric {
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub mem_rss: u64,
    pub cpu_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureMetric {
    pub sensor: String,
    pub temp_celsius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub load_1m: f64,
    pub load_5m: f64,
    pub load_15m: f64,
}

// ---------------------------------------------------------------------------
// Alert types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Warning,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warning => write!(f, "warning"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: i64,
    pub severity: AlertSeverity,
    pub category: String,
    pub message: String,
    pub value: String,
    pub threshold: String,
    pub first_seen: String,
    pub last_seen: String,
    pub count: u32,
    pub acknowledged: bool,
    pub acked_by: Option<String>,
    pub acked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub hours: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertAckRequest {
    pub alert_id: i64,
    pub acked_by: String,
}

// ---------------------------------------------------------------------------
// Database operations
// ---------------------------------------------------------------------------

/// Initialize monitor tables in the database.
pub fn init_db() -> Result<()> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS monitor_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS monitor_alerts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            severity TEXT NOT NULL,
            category TEXT NOT NULL,
            message TEXT NOT NULL,
            value TEXT NOT NULL DEFAULT '',
            threshold TEXT NOT NULL DEFAULT '',
            first_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
            last_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
            count INTEGER DEFAULT 1,
            acknowledged INTEGER DEFAULT 0,
            acked_by TEXT,
            acked_at DATETIME
        );

        CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON monitor_metrics(timestamp);
        CREATE INDEX IF NOT EXISTS idx_alerts_category ON monitor_alerts(category, acknowledged);
    ")?;

    info!("Monitor database tables initialized");
    Ok(())
}

/// Store a metrics snapshot.
fn store_metrics(metrics: &SystemMetrics) -> Result<()> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let json = serde_json::to_string(metrics).context("Failed to serialize metrics")?;
    conn.execute(
        "INSERT INTO monitor_metrics (timestamp, data) VALUES (?1, ?2)",
        rusqlite::params![metrics.timestamp, json],
    )?;
    Ok(())
}

/// Get the most recent metrics snapshot.
pub fn get_current_metrics() -> Result<Option<SystemMetrics>> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT data FROM monitor_metrics ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let data: String = row.get(0)?;
        let metrics: SystemMetrics = serde_json::from_str(&data)?;
        Ok(Some(metrics))
    } else {
        Ok(None)
    }
}

/// Get historical metrics for the last N hours.
pub fn get_history(hours: u32, limit: u32) -> Result<Vec<SystemMetrics>> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT data FROM monitor_metrics
         WHERE timestamp >= datetime('now', ?1)
         ORDER BY id DESC
         LIMIT ?2",
    )?;
    let hours_ago = format!("-{} hours", hours);
    let rows = stmt.query_map(rusqlite::params![hours_ago, limit], |row| {
        let data: String = row.get(0)?;
        Ok(data)
    })?;

    let mut results = Vec::new();
    for row in rows {
        if let Ok(data) = row {
            if let Ok(metrics) = serde_json::from_str::<SystemMetrics>(&data) {
                results.push(metrics);
            }
        }
    }
    Ok(results)
}

/// Prune old metrics (keep last 7 days).
fn prune_old_metrics() -> Result<()> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let deleted = conn.execute(
        "DELETE FROM monitor_metrics WHERE timestamp < datetime('now', '-7 days')",
        [],
    )?;
    if deleted > 0 {
        info!("Pruned {} old monitor metrics", deleted);
    }
    Ok(())
}

/// Get all active (unacknowledged) alerts.
pub fn get_active_alerts() -> Result<Vec<Alert>> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, severity, category, message, value, threshold,
                first_seen, last_seen, count, acknowledged, acked_by, acked_at
         FROM monitor_alerts
         WHERE acknowledged = 0
         ORDER BY
           CASE severity WHEN 'critical' THEN 0 WHEN 'warning' THEN 1 END,
           last_seen DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Alert {
            id: row.get(0)?,
            severity: {
                let s: String = row.get(1)?;
                match s.as_str() {
                    "critical" => AlertSeverity::Critical,
                    _ => AlertSeverity::Warning,
                }
            },
            category: row.get(2)?,
            message: row.get(3)?,
            value: row.get(4)?,
            threshold: row.get(5)?,
            first_seen: row.get(6)?,
            last_seen: row.get(7)?,
            count: row.get(8)?,
            acknowledged: row.get::<_, i64>(9)? != 0,
            acked_by: row.get(10)?,
            acked_at: row.get(11)?,
        })
    })?;

    let mut alerts = Vec::new();
    for row in rows {
        if let Ok(alert) = row {
            alerts.push(alert);
        }
    }
    Ok(alerts)
}

/// Acknowledge an alert.
pub fn acknowledge_alert(alert_id: i64, acked_by: &str) -> Result<bool> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();
    let updated = conn.execute(
        "UPDATE monitor_alerts
         SET acknowledged = 1, acked_by = ?1, acked_at = CURRENT_TIMESTAMP
         WHERE id = ?2 AND acknowledged = 0",
        rusqlite::params![acked_by, alert_id],
    )?;
    Ok(updated > 0)
}

// ---------------------------------------------------------------------------
// Alert engine
// ---------------------------------------------------------------------------

fn upsert_alert(
    conn: &rusqlite::Connection,
    severity: AlertSeverity,
    category: &str,
    message: &str,
    value: &str,
    threshold: &str,
) -> Result<()> {
    // Check if an unacknowledged alert for this category already exists
    let existing: Option<(i64, u32)> = {
        let mut stmt = conn.prepare(
            "SELECT id, count FROM monitor_alerts
             WHERE category = ?1 AND acknowledged = 0
             LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![category])?;
        if let Some(row) = rows.next()? {
            Some((row.get(0)?, row.get(1)?))
        } else {
            None
        }
    };

    if let Some((id, count)) = existing {
        conn.execute(
            "UPDATE monitor_alerts
             SET severity = ?1, message = ?2, value = ?3, threshold = ?4,
                 last_seen = CURRENT_TIMESTAMP, count = ?5
             WHERE id = ?6",
            rusqlite::params![severity.to_string(), message, value, threshold, count + 1, id],
        )?;
    } else {
        conn.execute(
            "INSERT INTO monitor_alerts (severity, category, message, value, threshold)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![severity.to_string(), category, message, value, threshold],
        )?;
    }
    Ok(())
}

fn check_alerts(metrics: &SystemMetrics) -> Result<()> {
    let db = crate::db::get();
    let conn = db.conn.lock().unwrap();

    // CPU > 90%
    if metrics.cpu_percent > 90.0 {
        upsert_alert(
            &conn,
            AlertSeverity::Critical,
            "cpu_high",
            &format!("CPU usage is {:.1}%", metrics.cpu_percent),
            &format!("{:.1}", metrics.cpu_percent),
            "90",
        )?;
    }

    // Memory > 85%
    if metrics.memory.percent > 85.0 {
        upsert_alert(
            &conn,
            AlertSeverity::Critical,
            "memory_high",
            &format!("Memory usage is {:.1}%", metrics.memory.percent),
            &format!("{:.1}", metrics.memory.percent),
            "85",
        )?;
    }

    // Disk > 90%
    for disk in &metrics.disk_usage {
        if disk.percent > 90.0 {
            upsert_alert(
                &conn,
                AlertSeverity::Critical,
                &format!("disk_high_{}", disk.mountpoint.replace('/', "_")),
                &format!("Disk {} ({}) is {:.1}% full", disk.device, disk.mountpoint, disk.percent),
                &format!("{:.1}", disk.percent),
                "90",
            )?;
        }
    }

    // Interface down
    for iface in &metrics.network {
        if iface.state != "up" && iface.state != "unknown" {
            upsert_alert(
                &conn,
                AlertSeverity::Warning,
                &format!("iface_down_{}", iface.name),
                &format!("Interface {} is {}", iface.name, iface.state),
                &iface.state,
                "up",
            )?;
        }
    }

    // PPPoE disconnect
    if metrics.vpp.available && metrics.vpp.pppoe_total > 0 && metrics.vpp.pppoe_active == 0 {
        upsert_alert(
            &conn,
            AlertSeverity::Critical,
            "pppoe_disconnect",
            "PPPoE session is disconnected but configured",
            "0",
            ">0",
        )?;
    }

    // VPP crash (VPP unavailable but was expected)
    if !metrics.vpp.available {
        // Only alert if VPP was previously available (check for existing alert)
        let has_prior = {
            let mut stmt = conn.prepare(
                "SELECT COUNT(*) FROM monitor_alerts
                 WHERE category = 'vpp_crash' AND acknowledged = 0",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count
        };
        if has_prior == 0 {
            upsert_alert(
                &conn,
                AlertSeverity::Critical,
                "vpp_crash",
                "VPP process is not responding",
                "unavailable",
                "available",
            )?;
        }
    }

    // VPP errors increasing
    if metrics.vpp.available && metrics.vpp.errors_total > 0 {
        upsert_alert(
            &conn,
            AlertSeverity::Warning,
            "vpp_errors",
            &format!("VPP has {} error counters", metrics.vpp.errors_total),
            &metrics.vpp.errors_total.to_string(),
            "0",
        )?;
    }

    // Process monitoring
    for proc in &metrics.processes {
        if !proc.running {
            // Only alert for critical processes
            if matches!(proc.name.as_str(), "vpp" | "dnsmasq" | "vectoros") {
                upsert_alert(
                    &conn,
                    AlertSeverity::Critical,
                    &format!("process_down_{}", proc.name),
                    &format!("Process {} is not running", proc.name),
                    "down",
                    "running",
                )?;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Collect metrics (calls Python monitor script)
// ---------------------------------------------------------------------------

const MONITOR_SCRIPT: &str = "/root/VectorOS/vpp-tools/monitor.py";

fn collect_metrics_from_script() -> Result<SystemMetrics> {
    let output = std::process::Command::new("python3")
        .arg(MONITOR_SCRIPT)
        .output()
        .context("Failed to execute monitor.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("monitor.py failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: serde_json::Value = serde_json::from_str(&stdout)
        .context("Failed to parse monitor.py JSON")?;

    let metrics = SystemMetrics {
        timestamp: Utc::now().to_rfc3339(),
        cpu_percent: raw["cpu"]["percent"].as_f64().unwrap_or(0.0),
        cpu_count: raw["cpu"]["count"].as_u64().unwrap_or(1) as u32,
        cpu_cores: parse_core_metrics(&raw["cpu_cores"]),
        memory: MemoryMetric {
            total: raw["memory"]["total"].as_u64().unwrap_or(0),
            used: raw["memory"]["used"].as_u64().unwrap_or(0),
            free: raw["memory"]["free"].as_u64().unwrap_or(0),
            buffers: raw["memory"]["buffers"].as_u64().unwrap_or(0),
            cached: raw["memory"]["cached"].as_u64().unwrap_or(0),
            available: raw["memory"]["available"].as_u64().unwrap_or(0),
            percent: raw["memory"]["percent"].as_f64().unwrap_or(0.0),
            swap_total: raw["memory"]["swap_total"].as_u64().unwrap_or(0),
            swap_used: raw["memory"]["swap_used"].as_u64().unwrap_or(0),
            swap_free: raw["memory"]["swap_free"].as_u64().unwrap_or(0),
            swap_percent: raw["memory"]["swap_percent"].as_f64().unwrap_or(0.0),
        },
        disk_usage: parse_disk_metrics(&raw["disk_usage"]),
        disk_io: DiskIoMetric {
            total_read_bytes_per_sec: raw["disk_io"]["total_read_bytes_per_sec"].as_u64().unwrap_or(0),
            total_write_bytes_per_sec: raw["disk_io"]["total_write_bytes_per_sec"].as_u64().unwrap_or(0),
            devices: parse_disk_io_devices(&raw["disk_io"]["devices"]),
        },
        network: parse_network_metrics(&raw["network"]),
        vpp: VppMetric {
            available: raw["vpp"]["available"].as_bool().unwrap_or(false),
            version: raw["vpp"]["version"].as_str().unwrap_or("").to_string(),
            nat_sessions: raw["vpp"]["nat_sessions"].as_u64().unwrap_or(0) as u32,
            pppoe_active: raw["vpp"]["pppoe"]["active"].as_u64().unwrap_or(0) as u32,
            pppoe_discovery: raw["vpp"]["pppoe"]["discovery"].as_u64().unwrap_or(0) as u32,
            pppoe_total: raw["vpp"]["pppoe"]["total"].as_u64().unwrap_or(0) as u32,
            memory_total_mb: raw["vpp"]["memory"]["total_mb"].as_f64().unwrap_or(0.0),
            memory_used_mb: raw["vpp"]["memory"]["used_mb"].as_f64().unwrap_or(0.0),
            memory_percent: raw["vpp"]["memory"]["percent"].as_f64().unwrap_or(0.0),
            errors_total: raw["vpp"]["errors"]["total"].as_u64().unwrap_or(0),
            worker_threads: raw["vpp"]["worker_threads"].as_u64().unwrap_or(0) as u32,
            packet_rate_rx: raw["vpp"]["packet_rate_rx"].as_u64().unwrap_or(0),
            packet_rate_tx: raw["vpp"]["packet_rate_tx"].as_u64().unwrap_or(0),
        },
        processes: parse_process_metrics(&raw["processes"]),
        temperatures: parse_temperature_metrics(&raw["temperatures"]),
        load_average: LoadAverage {
            load_1m: raw["load_average"]["load_1m"].as_f64().unwrap_or(0.0),
            load_5m: raw["load_average"]["load_5m"].as_f64().unwrap_or(0.0),
            load_15m: raw["load_average"]["load_15m"].as_f64().unwrap_or(0.0),
        },
        uptime: raw["uptime"].as_f64().unwrap_or(0.0),
    };

    Ok(metrics)
}

fn parse_core_metrics(val: &serde_json::Value) -> Vec<CoreMetric> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(CoreMetric {
                core: item["core"].as_u64().unwrap_or(0) as u32,
                percent: item["percent"].as_f64().unwrap_or(0.0),
            });
        }
    }
    result
}

fn parse_disk_metrics(val: &serde_json::Value) -> Vec<DiskMetric> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(DiskMetric {
                device: item["device"].as_str().unwrap_or("").to_string(),
                fstype: item["fstype"].as_str().unwrap_or("").to_string(),
                total: item["total"].as_u64().unwrap_or(0),
                used: item["used"].as_u64().unwrap_or(0),
                available: item["available"].as_u64().unwrap_or(0),
                percent: item["percent"].as_f64().unwrap_or(0.0),
                mountpoint: item["mountpoint"].as_str().unwrap_or("").to_string(),
                health: item["health"].as_str().unwrap_or("unknown").to_string(),
            });
        }
    }
    result
}

fn parse_disk_io_devices(val: &serde_json::Value) -> Vec<DiskIoDevice> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(DiskIoDevice {
                name: item["name"].as_str().unwrap_or("").to_string(),
                read_bytes_per_sec: item["read_bytes_per_sec"].as_u64().unwrap_or(0),
                write_bytes_per_sec: item["write_bytes_per_sec"].as_u64().unwrap_or(0),
            });
        }
    }
    result
}

fn parse_network_metrics(val: &serde_json::Value) -> Vec<NetworkMetric> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(NetworkMetric {
                name: item["name"].as_str().unwrap_or("").to_string(),
                state: item["state"].as_str().unwrap_or("unknown").to_string(),
                rx_bytes: item["rx_bytes"].as_u64().unwrap_or(0),
                tx_bytes: item["tx_bytes"].as_u64().unwrap_or(0),
                rx_packets: item["rx_packets"].as_u64().unwrap_or(0),
                tx_packets: item["tx_packets"].as_u64().unwrap_or(0),
                rx_errors: item["rx_errors"].as_u64().unwrap_or(0),
                tx_errors: item["tx_errors"].as_u64().unwrap_or(0),
                rx_drops: item["rx_drops"].as_u64().unwrap_or(0),
                tx_drops: item["tx_drops"].as_u64().unwrap_or(0),
                rx_bps: item["rx_bps"].as_u64().unwrap_or(0),
                tx_bps: item["tx_bps"].as_u64().unwrap_or(0),
                rx_pps: item["rx_pps"].as_u64().unwrap_or(0),
                tx_pps: item["tx_pps"].as_u64().unwrap_or(0),
            });
        }
    }
    result
}

fn parse_process_metrics(val: &serde_json::Value) -> Vec<ProcessMetric> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(ProcessMetric {
                name: item["name"].as_str().unwrap_or("").to_string(),
                running: item["running"].as_bool().unwrap_or(false),
                pid: item["pid"].as_u64().map(|p| p as u32),
                mem_rss: item["mem_rss"].as_u64().unwrap_or(0),
                cpu_percent: item["cpu_percent"].as_f64().unwrap_or(0.0),
            });
        }
    }
    result
}

fn parse_temperature_metrics(val: &serde_json::Value) -> Vec<TemperatureMetric> {
    let mut result = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            result.push(TemperatureMetric {
                sensor: item["sensor"].as_str().unwrap_or("").to_string(),
                temp_celsius: item["temp_celsius"].as_f64().unwrap_or(0.0),
            });
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Background collection loop
// ---------------------------------------------------------------------------

/// Start the background metrics collection loop.
pub async fn start_collector() {
    info!("Starting monitor collector (5 second interval)");

    // Initialize DB tables
    if let Err(e) = init_db() {
        error!("Failed to initialize monitor DB: {}", e);
        return;
    }

    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let mut prune_counter = 0u32;

    loop {
        interval.tick().await;

        match collect_metrics_from_script() {
            Ok(metrics) => {
                // Check alerts before storing
                if let Err(e) = check_alerts(&metrics) {
                    warn!("Alert check failed: {}", e);
                }

                // Store metrics
                if let Err(e) = store_metrics(&metrics) {
                    warn!("Failed to store metrics: {}", e);
                }

                // Prune old metrics every hour (720 ticks at 5s)
                prune_counter += 1;
                if prune_counter >= 720 {
                    prune_counter = 0;
                    if let Err(e) = prune_old_metrics() {
                        warn!("Failed to prune old metrics: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to collect metrics: {}", e);
            }
        }
    }
}

/// Compute a health score (0-100) from the latest metrics.
pub fn compute_health_score(metrics: &SystemMetrics) -> u32 {
    let mut score: i32 = 100;

    // CPU penalty
    if metrics.cpu_percent > 90.0 {
        score -= 30;
    } else if metrics.cpu_percent > 70.0 {
        score -= 15;
    } else if metrics.cpu_percent > 50.0 {
        score -= 5;
    }

    // Memory penalty
    if metrics.memory.percent > 90.0 {
        score -= 25;
    } else if metrics.memory.percent > 85.0 {
        score -= 15;
    } else if metrics.memory.percent > 70.0 {
        score -= 5;
    }

    // Disk penalty
    for disk in &metrics.disk_usage {
        if disk.percent > 95.0 {
            score -= 20;
        } else if disk.percent > 90.0 {
            score -= 10;
        } else if disk.percent > 80.0 {
            score -= 3;
        }
    }

    // VPP penalty
    if !metrics.vpp.available {
        score -= 30;
    } else if metrics.vpp.errors_total > 100 {
        score -= 10;
    }

    // Process penalty
    for proc in &metrics.processes {
        if !proc.running && matches!(proc.name.as_str(), "vpp" | "vectoros") {
            score -= 20;
        }
    }

    // Interface penalty
    for iface in &metrics.network {
        if iface.state != "up" && iface.state != "unknown" {
            score -= 5;
        }
    }

    score.max(0).min(100) as u32
}
