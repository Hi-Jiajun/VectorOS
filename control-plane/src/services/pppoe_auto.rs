//! PPPoE Auto-Connect Service
//!
//! Monitors PPPoE connection status and automatically reconnects on disconnect
//! with exponential backoff, health checks, and DNS refresh on reconnect.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::config::PppoeAutoConfig;

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Current status of the auto-connect service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoConnectStatus {
    Idle,
    Connecting,
    Connected,
    Retrying,
    Disabled,
    Failed,
}

impl std::fmt::Display for AutoConnectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Retrying => write!(f, "retrying"),
            Self::Disabled => write!(f, "disabled"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// A single connection history event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub event_type: String,
    pub message: String,
}

/// Full auto-connect status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeAutoStatus {
    pub enabled: bool,
    pub status: AutoConnectStatus,
    pub running: bool,
    pub consecutive_failures: u32,
    pub total_reconnects: u32,
    pub current_retry_interval: f64,
    pub last_connect_time: Option<String>,
    pub last_disconnect_time: Option<String>,
    pub last_health_check: Option<String>,
    pub config: PppoeAutoConfig,
    pub history: Vec<HistoryEntry>,
}

// ---------------------------------------------------------------------------
// Internal runtime state
// ---------------------------------------------------------------------------

struct RuntimeState {
    status: AutoConnectStatus,
    consecutive_failures: u32,
    total_reconnects: u32,
    current_retry_interval: f64,
    last_connect_time: Option<Instant>,
    last_disconnect_time: Option<Instant>,
    last_health_check: Option<Instant>,
    history: Vec<HistoryEntry>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            status: AutoConnectStatus::Idle,
            consecutive_failures: 0,
            total_reconnects: 0,
            current_retry_interval: 5.0,
            last_connect_time: None,
            last_disconnect_time: None,
            last_health_check: None,
            history: Vec::new(),
        }
    }

    fn add_event(&mut self, event_type: &str, message: &str) {
        let entry = HistoryEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: event_type.to_string(),
            message: message.to_string(),
        };
        self.history.push(entry);
        // Keep only last 100 entries
        if self.history.len() > 100 {
            self.history = self.history.split_off(self.history.len() - 100);
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-Connect Service
// ---------------------------------------------------------------------------

/// PPPoE auto-connect service that monitors connection status and reconnects
/// with exponential backoff.
pub struct PppoeAutoConnectService {
    config: RwLock<PppoeAutoConfig>,
    state: Arc<RwLock<RuntimeState>>,
    shutdown: Arc<tokio::sync::Notify>,
}

impl PppoeAutoConnectService {
    /// Create a new auto-connect service.
    pub fn new(initial_config: PppoeAutoConfig) -> Self {
        let mut state = RuntimeState::new();
        if !initial_config.enabled {
            state.status = AutoConnectStatus::Disabled;
        }
        state.current_retry_interval = initial_config.retry_interval as f64;

        Self {
            config: RwLock::new(initial_config),
            state: Arc::new(RwLock::new(state)),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Get a read-only copy of the current configuration.
    pub async fn get_config(&self) -> PppoeAutoConfig {
        self.config.read().await.clone()
    }

    /// Get a writable reference to the configuration.
    /// This returns a `RwLockWriteGuard` so the caller can mutate in place.
    pub async fn config_for_write(&self) -> tokio::sync::RwLockWriteGuard<'_, PppoeAutoConfig> {
        self.config.write().await
    }

    /// Start the auto-connect monitoring loop.
    pub async fn start(&self, pppoe_username: String, pppoe_password: String) -> Result<()> {
        let config = self.config.read().await.clone();
        if !config.enabled {
            return Ok(());
        }

        {
            let mut state = self.state.write().await;
            state.status = AutoConnectStatus::Connecting;
            state.add_event("started", "Auto-connect service started");
        }

        info!("PPPoE auto-connect starting (max_retries={}, check_interval={}s)",
              config.max_retries, config.check_interval);

        let state = self.state.clone();
        let shutdown = self.shutdown.clone();
        let cfg = config.clone();

        tokio::spawn(async move {
            Self::run_loop(state, shutdown, cfg, pppoe_username, pppoe_password).await;
        });

        Ok(())
    }

    /// Stop the auto-connect monitoring loop.
    pub async fn stop(&self) -> Result<()> {
        self.shutdown.notify_waiters();

        let mut state = self.state.write().await;
        state.status = AutoConnectStatus::Idle;
        state.add_event("stopped", "Auto-connect service stopped");

        info!("PPPoE auto-connect stopped");
        Ok(())
    }

    /// Update configuration at runtime.
    pub async fn update_config(&self, new_config: PppoeAutoConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
        info!("PPPoE auto-connect configuration updated");
    }

    /// Get current status.
    pub async fn get_status(&self) -> PppoeAutoStatus {
        let config = self.config.read().await.clone();
        let state = self.state.read().await;

        PppoeAutoStatus {
            enabled: config.enabled,
            status: state.status.clone(),
            running: state.status != AutoConnectStatus::Disabled
                && state.status != AutoConnectStatus::Idle,
            consecutive_failures: state.consecutive_failures,
            total_reconnects: state.total_reconnects,
            current_retry_interval: state.current_retry_interval,
            last_connect_time: state.last_connect_time.map(|t| {
                chrono::DateTime::from_timestamp(t.elapsed().as_secs() as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            }),
            last_disconnect_time: state.last_disconnect_time.map(|t| {
                chrono::DateTime::from_timestamp(t.elapsed().as_secs() as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            }),
            last_health_check: state.last_health_check.map(|t| {
                chrono::DateTime::from_timestamp(t.elapsed().as_secs() as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            }),
            config,
            history: state.history.clone(),
        }
    }

    /// Get connection history.
    pub async fn get_history(&self, limit: usize) -> Vec<HistoryEntry> {
        let state = self.state.read().await;
        state.history.iter().rev().take(limit).cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Internal monitoring loop
    // -----------------------------------------------------------------------

    async fn run_loop(
        state: Arc<RwLock<RuntimeState>>,
        shutdown: Arc<tokio::sync::Notify>,
        config: PppoeAutoConfig,
        username: String,
        password: String,
    ) {
        let mut current_retry_interval = config.retry_interval as f64;
        let mut consecutive_failures: u32 = 0;
        let mut last_health_check: Option<Instant> = None;

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Auto-connect loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(config.check_interval)) => {}
            }

            // Check if connected by running pppoe_manager.py dump
            let connected = match Self::check_connection().await {
                Ok(c) => c,
                Err(e) => {
                    warn!("Connection check failed: {}", e);
                    false
                }
            };

            if connected {
                let mut s = state.write().await;
                s.status = AutoConnectStatus::Connected;
                s.consecutive_failures = 0;
                s.current_retry_interval = config.retry_interval as f64;
                if s.last_connect_time.is_none() {
                    s.last_connect_time = Some(Instant::now());
                }
                current_retry_interval = config.retry_interval as f64;
                consecutive_failures = 0;

                // Health check
                let now = Instant::now();
                if last_health_check.is_none()
                    || now.duration_since(last_health_check.unwrap())
                        >= Duration::from_secs(config.health_check_interval)
                {
                    last_health_check = Some(now);
                    s.last_health_check = Some(now);
                    s.add_event("health_check_ok", "Connection healthy");
                }

                continue;
            }

            // Not connected -- attempt reconnect
            {
                let mut s = state.write().await;
                if s.status == AutoConnectStatus::Connected {
                    s.last_disconnect_time = Some(Instant::now());
                    s.add_event("disconnected", "PPPoE session lost");
                }
            }

            if config.max_retries > 0 && consecutive_failures >= config.max_retries {
                let mut s = state.write().await;
                s.status = AutoConnectStatus::Failed;
                s.add_event(
                    "max_retries_reached",
                    &format!("Max retries ({}) reached", config.max_retries),
                );
                error!("Max retries ({}) reached, stopping auto-connect", config.max_retries);
                break;
            }

            // Attempt connection
            {
                let mut s = state.write().await;
                s.status = AutoConnectStatus::Connecting;
                s.add_event(
                    "connect_attempt",
                    &format!("Attempt #{}", consecutive_failures + 1),
                );
            }

            match Self::attempt_connect(&username, &password).await {
                Ok(true) => {
                    consecutive_failures = 0;
                    current_retry_interval = config.retry_interval as f64;
                    let mut s = state.write().await;
                    s.status = AutoConnectStatus::Connected;
                    s.total_reconnects += 1;
                    s.last_connect_time = Some(Instant::now());
                    s.current_retry_interval = config.retry_interval as f64;
                    let total = s.total_reconnects;
                    s.add_event(
                        "connected",
                        &format!("Session established (total: {})", total),
                    );
                    info!("PPPoE reconnected (total reconnects: {})", total);

                    // DNS refresh
                    drop(s);
                    Self::refresh_dns().await;
                }
                Ok(false) => {
                    consecutive_failures += 1;
                    current_retry_interval =
                        (current_retry_interval * config.backoff_factor).min(config.max_retry_interval as f64);
                    let mut s = state.write().await;
                    s.status = AutoConnectStatus::Retrying;
                    s.consecutive_failures = consecutive_failures;
                    s.current_retry_interval = current_retry_interval;
                    s.add_event(
                        "connect_failed",
                        &format!("Attempt #{} failed", consecutive_failures),
                    );
                    warn!("Connect attempt #{} failed, retrying in {:.1}s",
                          consecutive_failures, current_retry_interval);
                }
                Err(e) => {
                    consecutive_failures += 1;
                    current_retry_interval =
                        (current_retry_interval * config.backoff_factor).min(config.max_retry_interval as f64);
                    let mut s = state.write().await;
                    s.status = AutoConnectStatus::Retrying;
                    s.consecutive_failures = consecutive_failures;
                    s.current_retry_interval = current_retry_interval;
                    s.add_event("error", &e.to_string());
                    error!("Connect error: {} (attempt {})", e, consecutive_failures);
                }
            }

            // Wait for retry interval (with shutdown check)
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Auto-connect loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(current_retry_interval as u64)) => {}
            }
        }
    }

    /// Check if PPPoE session is currently active.
    async fn check_connection() -> Result<bool> {
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new("python3")
                .arg("/root/VectorOS/vpp-tools/pppoe_manager.py")
                .arg("dump")
                .output()
        })
        .await??;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let data: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

        // Check if any client has state == 3 (SESSION)
        if let Some(clients) = data.as_array() {
            for client in clients {
                if client.get("client_state").and_then(|v| v.as_u64()) == Some(3) {
                    return Ok(true);
                }
            }
        } else if let Some(clients) = data.get("clients").and_then(|v| v.as_array()) {
            for client in clients {
                if client.get("client_state").and_then(|v| v.as_u64()) == Some(3) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Attempt to create a PPPoE session.
    async fn attempt_connect(username: &str, password: &str) -> Result<bool> {
        let username = username.to_string();
        let password = password.to_string();

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("python3")
                .arg("/root/VectorOS/vpp-tools/pppoe_manager.py")
                .arg("create")
                .arg("--sw-if-index").arg("1")
                .arg("--username").arg(&username)
                .arg("--password").arg(&password)
                .arg("--mtu").arg("1492")
                .arg("--mru").arg("1492")
                .output()
        })
        .await??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("PPPoE create failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let data: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

        if data.get("error").is_some() {
            return Ok(false);
        }

        Ok(true)
    }

    /// Refresh DNS after reconnection.
    async fn refresh_dns() {
        match tokio::task::spawn_blocking(|| {
            std::process::Command::new("systemctl")
                .arg("restart")
                .arg("vectoros-dns")
                .output()
        })
        .await
        {
            Ok(Ok(output)) => {
                if output.status.success() {
                    info!("DNS refreshed after reconnection");
                } else {
                    warn!("DNS refresh returned non-zero exit");
                }
            }
            Ok(Err(e)) => warn!("DNS refresh command failed: {}", e),
            Err(e) => warn!("DNS refresh task failed: {}", e),
        }
    }
}
