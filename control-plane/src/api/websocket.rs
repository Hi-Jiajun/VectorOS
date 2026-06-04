use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn, error};

use crate::api::AppState;

/// WebSocket message types for real-time dashboard updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// System statistics update
    SystemUpdate {
        cpu_percent: f64,
        cpu_count: u32,
        memory_total: u64,
        memory_used: u64,
        memory_percent: f64,
        disk_total: u64,
        disk_used: u64,
        disk_percent: f64,
    },
    /// VPP performance metrics update
    VppUpdate {
        packet_rate_rx: f64,
        packet_rate_tx: f64,
        nat_sessions: u32,
        pppoe_status: String,
        interfaces: Vec<VppInterfaceStats>,
    },
    /// Interface status update
    InterfaceUpdate {
        name: String,
        state: String,
        rx_bytes: u64,
        tx_bytes: u64,
        rx_packets: u64,
        tx_packets: u64,
    },
    /// Alert/notification update
    AlertUpdate {
        level: AlertLevel,
        message: String,
        timestamp: String,
    },
    /// Connection established confirmation
    Connected {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VppInterfaceStats {
    pub name: String,
    pub state: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
}

/// Handle WebSocket upgrade requests
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle individual WebSocket connections
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.ws_tx.subscribe();

    // Send connection confirmation
    let connected_msg = WsMessage::Connected {
        message: "Connected to VectorOS real-time updates".to_string(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            error!("Failed to send connection confirmation");
            return;
        }
    }

    info!("WebSocket client connected");

    // Spawn task to forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    warn!("Failed to send WebSocket message to client");
                    break;
                }
            }
        }
    });

    // Spawn task to handle incoming messages from this client
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Handle client messages (e.g., subscription preferences)
                    info!("Received client message: {}", text);
                }
                Message::Close(_) => {
                    info!("WebSocket client disconnected");
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    info!("WebSocket connection closed");
}

/// Broadcast a message to all connected WebSocket clients
pub fn broadcast(ws_tx: &broadcast::Sender<WsMessage>, msg: WsMessage) {
    if let Err(e) = ws_tx.send(msg) {
        warn!("Failed to broadcast WebSocket message: {}", e);
    }
}

/// Background task that periodically collects and broadcasts system stats
pub async fn stats_broadcaster(ws_tx: broadcast::Sender<WsMessage>) {
    info!("Starting WebSocket stats broadcaster (2 second interval)");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));

    loop {
        interval.tick().await;

        // Collect system info and VPP performance in parallel using blocking tasks
        let system_info_handle = tokio::task::spawn_blocking(|| crate::vpp::native::get_system_info());
        let vpp_perf_handle = tokio::task::spawn_blocking(|| crate::vpp::native::get_vpp_performance());
        let pppoe_handle = tokio::task::spawn_blocking(|| crate::vpp::native::get_pppoe_status());

        match system_info_handle.await {
            Ok(Ok(info)) => {
                let system_update = WsMessage::SystemUpdate {
                    cpu_percent: info.cpu_percent,
                    cpu_count: info.cpu_count,
                    memory_total: info.memory_total,
                    memory_used: info.memory_used,
                    memory_percent: info.memory_percent,
                    disk_total: info.disk_total,
                    disk_used: info.disk_used,
                    disk_percent: info.disk_percent,
                };
                broadcast(&ws_tx, system_update);

                // Collect VPP performance metrics
                if let Ok(Ok(perf)) = vpp_perf_handle.await {
                    // Get PPPoE status
                    let pppoe_status = match pppoe_handle.await {
                        Ok(Ok(status)) => {
                            if status.clients.is_empty() {
                                "disconnected".to_string()
                            } else {
                                let active = status.clients.iter().filter(|c| c.client_state == 2).count();
                                format!("{} clients ({} active)", status.clients.len(), active)
                            }
                        }
                        _ => "unknown".to_string(),
                    };

                    // Convert interface throughput to our WebSocket format
                    let interfaces: Vec<VppInterfaceStats> = perf.interfaces.iter().map(|iface| {
                        VppInterfaceStats {
                            name: iface.name.clone(),
                            state: "up".to_string(), // Throughput data implies interface is up
                            rx_bytes: iface.rx_bytes,
                            tx_bytes: iface.tx_bytes,
                        }
                    }).collect();

                    let vpp_update = WsMessage::VppUpdate {
                        packet_rate_rx: perf.packet_rate.rx_packets_per_sec,
                        packet_rate_tx: perf.packet_rate.tx_packets_per_sec,
                        nat_sessions: perf.nat.session_count,
                        pppoe_status,
                        interfaces,
                    };
                    broadcast(&ws_tx, vpp_update);
                }
            }
            Ok(Err(e)) => {
                warn!("System info task returned error: {}", e);
            }
            Err(e) => {
                warn!("Failed to collect system stats for WebSocket broadcast: {}", e);
            }
        }
    }
}

/// Start the WebSocket stats broadcaster as a background task
pub fn start_stats_broadcaster(ws_tx: broadcast::Sender<WsMessage>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(stats_broadcaster(ws_tx))
}
