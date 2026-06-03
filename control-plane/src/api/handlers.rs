use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;
use crate::api::AppState;
use crate::vpp::pppoe::PppoeApi;

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

pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

pub async fn get_dhcp_leases() -> Json<Value> {
    // TODO: Query DHCP server for active leases
    Json(json!({ "leases": [] }))
}
