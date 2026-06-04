use axum::extract::{State, Path};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::Arc;
use crate::api::AppState;

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

/// Run a Python VPP command
fn run_vpp_cmd(action: &str, args: &[(&str, &str)]) -> Result<Value, String> {
    let mut cmd = Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/pppoe_manager.py");
    cmd.arg(action);

    for (key, value) in args {
        cmd.arg(format!("--{}", key));
        cmd.arg(value);
    }

    let output = cmd.output().map_err(|e| format!("Failed to run command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse output: {}", e))
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "config": state.config }))
}

pub async fn get_interfaces() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/interface_manager.py");
    cmd.arg("list");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn iface_up(Path(name): Path<String>) -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/interface_manager.py");
    cmd.arg("up").arg("--name").arg(&name);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn iface_down(Path(name): Path<String>) -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/interface_manager.py");
    cmd.arg("down").arg("--name").arg(&name);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_pppoe_clients() -> Json<Value> {
    match run_vpp_cmd("dump", &[]) {
        Ok(data) => Json(json!({ "clients": data })),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn get_pppoe_status() -> Json<Value> {
    match run_vpp_cmd("status", &[]) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn create_pppoe_client(
    Json(config): Json<PppoeConfig>,
) -> Json<Value> {
    // Map interface name to sw_if_index
    let sw_if_index = match config.interface.as_str() {
        "enp1s0" => "1",
        "enp2s0" => "2",
        "enp3s0" => "3",
        _ => return Json(json!({ "error": format!("Unknown interface: {}", config.interface) })),
    };

    let mtu_str = config.mtu.to_string();
    let mru_str = config.mru.to_string();

    let args = vec![
        ("sw-if-index", sw_if_index),
        ("username", &config.username),
        ("password", &config.password),
        ("mtu", &mtu_str),
        ("mru", &mru_str),
    ];

    match run_vpp_cmd("create", &args) {
        Ok(data) => Json(data),
        Err(e) => Json(json!({ "error": e })),
    }
}

pub async fn get_nat_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/nat_manager.py");
    cmd.arg("show");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn enable_nat() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/nat_manager.py");
    cmd.arg("enable");
    cmd.arg("--inside-if").arg("2");
    cmd.arg("--outside-if").arg("4");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_routes() -> Json<Value> {
    // TODO: Query VPP for routing table
    Json(json!({ "routes": [] }))
}

pub async fn get_system_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/system_monitor.py");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_config_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/config_manager.py");
    cmd.arg("get");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn save_config(Json(config): Json<Value>) -> Json<Value> {
    let config_str = serde_json::to_string(&config).unwrap_or_default();

    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/config_manager.py");
    cmd.arg("set");
    cmd.arg("--section").arg("all");
    cmd.arg("--key").arg("config");
    cmd.arg("--value").arg(&config_str);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_dns_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/dns_manager.py");
    cmd.arg("show");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn enable_dns() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/dns_manager.py");
    cmd.arg("enable");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn get_dhcp_status() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/dhcp_manager.py");
    cmd.arg("show");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}

pub async fn enable_dhcp() -> Json<Value> {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("/root/VectorOS/vpp-tools/dhcp_manager.py");
    cmd.arg("enable");
    cmd.arg("--interface").arg("lan0");
    cmd.arg("--start-ip").arg("192.168.1.100");
    cmd.arg("--end-ip").arg("192.168.1.200");
    cmd.arg("--gateway").arg("192.168.1.1");

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => Json(data),
                Err(e) => Json(json!({ "error": format!("Parse error: {}", e) })),
            }
        }
        Err(e) => Json(json!({ "error": format!("Command error: {}", e) })),
    }
}
