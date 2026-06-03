use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;
use crate::api::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "config": state.config }))
}

pub async fn get_interfaces() -> Json<Value> {
    // TODO: Query VPP for interface list
    Json(json!({ "interfaces": [] }))
}

pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

pub async fn get_dhcp_leases() -> Json<Value> {
    // TODO: Query DHCP server for active leases
    Json(json!({ "leases": [] }))
}
