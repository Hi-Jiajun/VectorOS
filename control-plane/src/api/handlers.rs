use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::api::AppState;
use crate::vpp::pppoe::PppoeApi;

#[derive(Debug, Deserialize)]
pub struct PppoeConfig {
    pub username: String,
    pub password: String,
    pub interface: String,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
    #[serde(default = "default_mru")]
    pub mru: u32,
    #[serde(default)]
    pub use_peer_dns: bool,
    #[serde(default)]
    pub add_default_route4: bool,
    #[serde(default)]
    pub add_default_route6: bool,
}

fn default_mtu() -> u32 { 1492 }
fn default_mru() -> u32 { 1492 }

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "config": state.config }))
}

pub async fn get_interfaces(State(state): State<Arc<AppState>>) -> Json<Value> {
    if let Some(ref vpp) = state.vpp {
        match vpp.get_interfaces() {
            Ok(ifaces) => Json(json!({ "interfaces": ifaces })),
            Err(e) => Json(json!({ "error": e.to_string() })),
        }
    } else {
        Json(json!({ "error": "VPP not connected" }))
    }
}

pub async fn get_pppoe_clients(State(state): State<Arc<AppState>>) -> Json<Value> {
    if let Some(ref vpp) = state.vpp {
        match vpp.pppoe_api() {
            Ok(pppoe) => {
                let base_id = pppoe.base_msg_id();
                Json(json!({
                    "status": "ok",
                    "base_msg_id": base_id,
                    "message": "PPPoE API initialized successfully"
                }))
            },
            Err(e) => Json(json!({ "error": format!("Failed to init PPPoE API: {}", e) })),
        }
    } else {
        Json(json!({ "error": "VPP not connected" }))
    }
}

pub async fn create_pppoe_client(
    State(state): State<Arc<AppState>>,
    Json(config): Json<PppoeConfig>,
) -> Json<Value> {
    let Some(ref vpp) = state.vpp else {
        return Json(json!({ "error": "VPP not connected" }));
    };

    let pppoe = match vpp.pppoe_api() {
        Ok(api) => api,
        Err(e) => return Json(json!({ "error": format!("PPPoE API init failed: {}", e) })),
    };

    // Get sw_if_index for the interface
    // For now, use a mapping. In production, query VPP for interface list
    let sw_if_index = match config.interface.as_str() {
        "enp1s0" => 1,
        "enp2s0" => 2,
        "enp3s0" => 3,
        _ => return Json(json!({ "error": format!("Unknown interface: {}", config.interface) })),
    };

    // Create PPPoE client
    match vpp.pppoe_add_client(
        &pppoe,
        sw_if_index,
        1, // host_uniq
        "", // ac_name (any)
        "", // service_name (any)
        "pppoe-wan0", // custom interface name
    ) {
        Ok(pppox_sw_if_index) => {
            // Set options (username, password, etc.)
            // TODO: Implement set_options call
            Json(json!({
                "status": "ok",
                "message": "PPPoE client created",
                "pppox_sw_if_index": pppox_sw_if_index,
                "config": {
                    "username": config.username,
                    "interface": config.interface,
                    "mtu": config.mtu,
                    "mru": config.mru
                }
            }))
        }
        Err(e) => Json(json!({ "error": format!("Failed to create PPPoE client: {}", e) })),
    }
}

pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

pub async fn get_dhcp_leases() -> Json<Value> {
    // TODO: Query DHCP server for active leases
    Json(json!({ "leases": [] }))
}
