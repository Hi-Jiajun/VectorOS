//! Service Manager — lifecycle orchestrator for VectorOS services
//!
//! Inspired by Landscape's `ServiceManager<Starter>` pattern.
//! Each managed service follows a strict state machine:
//!
//!   Stopped → Starting → Running → Stopping → Stopped
//!                     ↘ Failed ↗
//!
//! The manager supports automatic rollback on failed restarts and
//! hot-reload when configuration changes in the database.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Service lifecycle states
// ---------------------------------------------------------------------------

/// Lifecycle state of a managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    /// Service is not running.
    Stopped,
    /// Service is in the process of starting.
    Starting,
    /// Service is running and healthy.
    Running,
    /// Service is in the process of stopping.
    Stopping,
    /// Service has failed to start or has crashed.
    Failed,
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Stopping => write!(f, "stopping"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Service metadata
// ---------------------------------------------------------------------------

/// Snapshot of a single service's state, returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Machine-readable service identifier (e.g. "dhcp", "dns").
    pub name: String,
    /// Human-readable label (e.g. "DHCP Server").
    pub display_name: String,
    /// Current lifecycle state.
    pub state: ServiceState,
    /// Brief description of the service.
    pub description: String,
    /// ISO-8601 timestamp of the last state transition.
    pub last_transition: String,
    /// Optional error message when state == Failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Service trait
// ---------------------------------------------------------------------------

/// A manageable service that the `ServiceManager` can drive.
///
/// Implementors wrap one of the existing service modules and translate the
/// generic start/stop/status calls into module-specific operations.
#[async_trait::async_trait]
pub trait Service: Send + Sync {
    /// Unique identifier for this service (matches `ServiceInfo::name`).
    fn name(&self) -> &str;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// Short description.
    fn description(&self) -> &str;

    /// Start the service.  Implementations should apply configuration and
    /// bring the service to a running state.  Return `Err` on failure.
    async fn start(&self) -> Result<()>;

    /// Stop the service gracefully.
    async fn stop(&self) -> Result<()>;

    /// Restart the service (stop then start).  The default implementation
    /// calls `stop` followed by `start`.  Override for optimized restarts.
    async fn restart(&self) -> Result<()> {
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    /// Probe the *actual* runtime state of the service (e.g. check if a
    /// process is running).  Returns the observed state.
    async fn probe(&self) -> ServiceState;

    /// Reload configuration without restarting (hot-reload).
    /// Default is a full restart.
    async fn reload(&self) -> Result<()> {
        self.restart().await
    }
}

// ---------------------------------------------------------------------------
// ServiceManager
// ---------------------------------------------------------------------------

/// Central service orchestrator.
///
/// Holds a map of `name → (Box<dyn Service>, ServiceInfo)` and exposes
/// async methods for start/stop/restart/status with state-machine
/// enforcement, logging, and automatic rollback.
pub struct ServiceManager {
    services: RwLock<HashMap<String, ManagedService>>,
}

struct ManagedService {
    service: Box<dyn Service>,
    info: ServiceInfo,
}

impl ServiceManager {
    /// Create an empty manager.
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    /// Register a service with the manager.
    pub async fn register(&self, svc: Box<dyn Service>) {
        let name = svc.name().to_string();
        let info = ServiceInfo {
            name: name.clone(),
            display_name: svc.display_name().to_string(),
            state: ServiceState::Stopped,
            description: svc.description().to_string(),
            last_transition: chrono::Utc::now().to_rfc3339(),
            error: None,
        };
        info!("Service '{}' registered", name);
        self.services
            .write()
            .await
            .insert(name, ManagedService { service: svc, info });
    }

    /// List all services with their current info.
    pub async fn list_services(&self) -> Vec<ServiceInfo> {
        let services = self.services.read().await;
        services.values().map(|m| m.info.clone()).collect()
    }

    /// Get info for a single service by name.
    pub async fn get_service(&self, name: &str) -> Option<ServiceInfo> {
        let services = self.services.read().await;
        services.get(name).map(|m| m.info.clone())
    }

    /// Start a service by name.
    pub async fn start_service(&self, name: &str) -> Result<ServiceInfo> {
        // Validate preconditions
        {
            let services = self.services.read().await;
            let managed = services
                .get(name)
                .with_context(|| format!("Service '{}' not found", name))?;

            match managed.info.state {
                ServiceState::Running => {
                    return Ok(managed.info.clone());
                }
                ServiceState::Starting => {
                    return Ok(managed.info.clone());
                }
                _ => {}
            }
        }

        // Transition to Starting
        self.set_state(name, ServiceState::Starting, None).await;

        // Attempt start
        let result = {
            let services = self.services.read().await;
            if let Some(managed) = services.get(name) {
                managed.service.start().await
            } else {
                anyhow::bail!("Service '{}' disappeared during start", name);
            }
        };

        match result {
            Ok(()) => {
                // Verify via probe
                let actual_state = {
                    let services = self.services.read().await;
                    if let Some(managed) = services.get(name) {
                        managed.service.probe().await
                    } else {
                        ServiceState::Failed
                    }
                };

                if actual_state == ServiceState::Running {
                    self.set_state(name, ServiceState::Running, None).await;
                    info!("Service '{}' started successfully", name);
                } else {
                    // Probe says not running despite successful start call
                    self.set_state(
                        name,
                        ServiceState::Failed,
                        Some("Service did not reach running state after start".into()),
                    )
                    .await;
                    warn!("Service '{}' start returned ok but probe shows {}", name, actual_state);
                }
            }
            Err(e) => {
                let msg = e.to_string();
                error!("Service '{}' failed to start: {}", name, msg);
                self.set_state(name, ServiceState::Failed, Some(msg)).await;
                return Err(e);
            }
        }

        self.get_service(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Service '{}' disappeared", name))
    }

    /// Stop a service by name.
    pub async fn stop_service(&self, name: &str) -> Result<ServiceInfo> {
        {
            let services = self.services.read().await;
            let managed = services
                .get(name)
                .with_context(|| format!("Service '{}' not found", name))?;

            match managed.info.state {
                ServiceState::Stopped => {
                    return Ok(managed.info.clone());
                }
                ServiceState::Stopping => {
                    return Ok(managed.info.clone());
                }
                _ => {}
            }
        }

        self.set_state(name, ServiceState::Stopping, None).await;

        let result = {
            let services = self.services.read().await;
            if let Some(managed) = services.get(name) {
                managed.service.stop().await
            } else {
                Ok(())
            }
        };

        match result {
            Ok(()) => {
                self.set_state(name, ServiceState::Stopped, None).await;
                info!("Service '{}' stopped", name);
            }
            Err(e) => {
                let msg = e.to_string();
                warn!("Service '{}' stop returned error (forced to stopped): {}", name, msg);
                self.set_state(name, ServiceState::Stopped, Some(msg)).await;
            }
        }

        self.get_service(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Service '{}' disappeared", name))
    }

    /// Restart a service with automatic rollback on failure.
    ///
    /// 1. Remember current state.
    /// 2. Stop the service.
    /// 3. Start the service.
    /// 4. If start fails, attempt to restore the previous running state.
    pub async fn restart_service(&self, name: &str) -> Result<ServiceInfo> {
        let previous_state = {
            let services = self.services.read().await;
            services
                .get(name)
                .with_context(|| format!("Service '{}' not found", name))?
                .info
                .state
        };

        info!("Restarting service '{}' (previous state: {})", name, previous_state);

        // Stop first (ignore errors — we want to start fresh)
        let _ = self.stop_service(name).await;

        // Start
        match self.start_service(name).await {
            ok @ Ok(_) => ok,
            Err(start_err) => {
                // Rollback: if the service was previously running, try to
                // bring it back up.
                if previous_state == ServiceState::Running {
                    warn!(
                        "Rollback: attempting to restore service '{}' after failed restart",
                        name
                    );
                    if let Err(rb_err) = self.start_service(name).await {
                        error!(
                            "Rollback failed for '{}': {} (original error: {})",
                            name, rb_err, start_err
                        );
                    }
                }
                Err(start_err)
            }
        }
    }

    /// Get the status of a service.
    pub async fn status(&self, name: &str) -> Result<ServiceInfo> {
        let services = self.services.read().await;
        let managed = services
            .get(name)
            .with_context(|| format!("Service '{}' not found", name))?;
        Ok(managed.info.clone())
    }

    /// Reload configuration for a service (hot-reload).
    pub async fn reload_service(&self, name: &str) -> Result<ServiceInfo> {
        {
            let services = self.services.read().await;
            let managed = services
                .get(name)
                .with_context(|| format!("Service '{}' not found", name))?;

            if managed.info.state != ServiceState::Running {
                anyhow::bail!(
                    "Service '{}' must be running to reload (current state: {})",
                    name,
                    managed.info.state
                );
            }
        }

        let result = {
            let services = self.services.read().await;
            if let Some(managed) = services.get(name) {
                managed.service.reload().await
            } else {
                Ok(())
            }
        };

        match result {
            Ok(()) => {
                info!("Service '{}' reloaded", name);
            }
            Err(e) => {
                warn!("Service '{}' reload failed: {}", name, e);
            }
        }

        self.get_service(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Service '{}' disappeared", name))
    }

    /// Synchronize all services with their actual probe state.
    /// Useful at startup or after a crash recovery.
    pub async fn sync_all(&self) {
        let names: Vec<String> = {
            let services = self.services.read().await;
            services.keys().cloned().collect()
        };

        for name in &names {
            let actual_state = {
                let services = self.services.read().await;
                if let Some(managed) = services.get(name) {
                    managed.service.probe().await
                } else {
                    continue;
                }
            };

            let current_state = {
                let services = self.services.read().await;
                services.get(name).map(|m| m.info.state)
            };

            if Some(actual_state) != current_state {
                info!(
                    "Syncing service '{}': {} -> {}",
                    name,
                    current_state.map(|s| s.to_string()).unwrap_or_default(),
                    actual_state
                );
                self.set_state(name, actual_state, None).await;
            }
        }
    }

    // -- internal helpers --------------------------------------------------

    async fn set_state(&self, name: &str, state: ServiceState, error: Option<String>) {
        let mut services = self.services.write().await;
        if let Some(managed) = services.get_mut(name) {
            managed.info.state = state;
            managed.info.last_transition = chrono::Utc::now().to_rfc3339();
            managed.info.error = error;
        }
    }
}

// Re-export async_trait so service implementations can use it.
pub use async_trait;
