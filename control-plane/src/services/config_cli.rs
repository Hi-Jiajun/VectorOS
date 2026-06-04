//! VyOS-style hierarchical configuration management service
//!
//! Provides tree-structured configuration with set/delete commands,
//! commit/rollback system, config diff, templates, and version history.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
const CONFIG_DIR: &str = "/etc/vectoros";
const CONFIG_FILE: &str = "/etc/vectoros/config.json";
const STAGING_FILE: &str = "/etc/vectoros/.config_staging.json";
const HISTORY_DIR: &str = "/etc/vectoros/config_history";
const TEMPLATE_DIR: &str = "/etc/vectoros/config_templates";

// ── Configuration tree (JSON value as tree) ──────────────────────

/// The configuration tree is stored as `serde_json::Value` for maximum flexibility.
/// Paths are expressed as dot-separated strings (e.g. "interfaces.eth0.address").
pub type ConfigTree = serde_json::Value;

// ── Staged / committed state ────────────────────────────────────

/// Global state holding committed config, staging area, and CLI sessions.
pub struct ConfigCliState {
    committed: Mutex<ConfigTree>,
    staging: Mutex<Option<ConfigTree>>,
    sessions: Mutex<HashMap<String, CliSession>>,
}

/// A CLI session tracks the current working context for a connected user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSession {
    pub id: String,
    pub mode: CliMode,
    pub cwd: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CliMode {
    Operational,
    Configuration,
}

// ── History ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub version: String,
    pub timestamp: String,
    pub message: String,
    pub config: ConfigTree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub version: String,
    pub timestamp: String,
    pub message: String,
}

// ── Templates ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigTemplate {
    pub name: String,
    pub description: String,
    pub created: String,
    pub config: ConfigTree,
}

// ── Diff ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDiffEntry {
    pub op: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<serde_json::Value>,
}

// ── CLI Command result ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliResult {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Default configuration ────────────────────────────────────────

fn default_config() -> ConfigTree {
    serde_json::json!({
        "interfaces": {
            "eth0": {
                "state": "up",
                "mtu": 1500,
                "address": []
            },
            "eth1": {
                "state": "up",
                "mtu": 1500,
                "address": ["192.168.1.1/24"]
            }
        },
        "pppoe": {
            "enabled": false,
            "username": "",
            "password": "",
            "interface": "eth0",
            "mtu": 1492,
            "mru": 1492,
            "use_peer_dns": true,
            "add_default_route4": true,
            "add_default_route6": true
        },
        "dhcp": {
            "enabled": false,
            "interface": "eth1",
            "start_ip": "192.168.1.100",
            "end_ip": "192.168.1.200",
            "gateway": "192.168.1.1",
            "lease_time": 86400
        },
        "dns": {
            "enabled": false,
            "upstream": ["8.8.8.8", "1.1.1.1"],
            "cache_size": 1000
        },
        "nat": {
            "enabled": false,
            "inside_if": "eth1",
            "outside_if": "eth0"
        },
        "firewall": {
            "enabled": false,
            "rules": []
        },
        "ipv6": {
            "enabled": false,
            "lan_prefix": "2001:db8:1::/64",
            "lan_address": "2001:db8:1::1/64",
            "wan_prefix": "2001:db8:2::/64",
            "upstream_dns": ["2001:4860:4860::8888", "2606:4700:4700::1111"]
        },
        "vpn": {},
        "qos": {
            "enabled": false,
            "policers": {}
        },
        "traffic": {
            "enabled": false
        },
        "frr": {
            "enabled": false
        }
    })
}

// ── Implementation ───────────────────────────────────────────────

impl ConfigCliState {
    pub fn new() -> Self {
        let committed = Self::load_committed();
        Self {
            committed: Mutex::new(committed),
            staging: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn load_committed() -> ConfigTree {
        let path = Path::new(CONFIG_FILE);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = serde_json::from_str::<ConfigTree>(&content) {
                    return config;
                }
            }
        }
        default_config()
    }

    fn save_committed(&self, config: &ConfigTree) -> Result<()> {
        fs::create_dir_all(CONFIG_DIR).context("Failed to create config dir")?;
        let json = serde_json::to_string_pretty(config)?;
        fs::write(CONFIG_FILE, &json).context("Failed to write config file")?;
        Ok(())
    }

    fn save_staging(&self, config: &ConfigTree) -> Result<()> {
        fs::create_dir_all(CONFIG_DIR).context("Failed to create config dir")?;
        let json = serde_json::to_string_pretty(config)?;
        fs::write(STAGING_FILE, &json).context("Failed to write staging file")?;
        Ok(())
    }

    fn load_staging(&self) -> Option<ConfigTree> {
        let path = Path::new(STAGING_FILE);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = serde_json::from_str::<ConfigTree>(&content) {
                    return Some(config);
                }
            }
        }
        None
    }

    fn clear_staging(&self) {
        let _ = fs::remove_file(STAGING_FILE);
    }

    // ── Session management ──────────────────────────────────────

    pub fn create_session(&self) -> CliSession {
        let id = uuid_simple();
        let session = CliSession {
            id: id.clone(),
            mode: CliMode::Operational,
            cwd: Vec::new(),
            created_at: now_iso(),
        };
        self.sessions.lock().unwrap().insert(id.clone(), session.clone());
        session
    }

    pub fn get_session(&self, id: &str) -> Option<CliSession> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    pub fn set_session_mode(&self, id: &str, mode: CliMode) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.mode = mode;
            Ok(())
        } else {
            anyhow::bail!("Session not found")
        }
    }

    pub fn delete_session(&self, id: &str) {
        self.sessions.lock().unwrap().remove(id);
    }

    // ── CLI command execution ───────────────────────────────────

    /// Execute a CLI command string (VyOS-style).
    pub fn execute_command(&self, session_id: &str, input: &str) -> CliResult {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        if parts.is_empty() {
            return Self::cli_ok("Welcome to VectorOS CLI");
        }

        let session = match self.get_session(session_id) {
            Some(s) => s,
            None => return Self::cli_error("Invalid session"),
        };

        match parts[0] {
            // Mode switching
            "configure" | "conf" => {
                let _ = self.set_session_mode(session_id, CliMode::Configuration);
                Self::cli_ok("Entering configuration mode")
            }
            "exit" | "quit" => {
                if session.mode == CliMode::Configuration {
                    let _ = self.set_session_mode(session_id, CliMode::Operational);
                    Self::cli_ok("Exiting configuration mode")
                } else {
                    self.delete_session(session_id);
                    Self::cli_ok("Goodbye")
                }
            }

            // Operational mode commands
            "show" => self.cmd_show(&parts[1..], &session),

            // Configuration mode commands
            "set" if session.mode == CliMode::Configuration => {
                self.cmd_set(&parts[1..])
            }
            "delete" if session.mode == CliMode::Configuration => {
                self.cmd_delete(&parts[1..])
            }
            "commit" if session.mode == CliMode::Configuration => {
                self.cmd_commit()
            }
            "rollback" if session.mode == CliMode::Configuration => {
                self.cmd_rollback(&parts[1..])
            }
            "discard" if session.mode == CliMode::Configuration => {
                self.cmd_discard()
            }
            "diff" if session.mode == CliMode::Configuration => {
                self.cmd_diff(&parts[1..])
            }
            "save" if session.mode == CliMode::Configuration => {
                self.cmd_save_template(&parts[1..])
            }
            "load" if session.mode == CliMode::Configuration => {
                self.cmd_load_template(&parts[1..])
            }

            _ => {
                if session.mode == CliMode::Configuration {
                    Self::cli_error(&format!("Unknown command: {}. Available: set, delete, commit, rollback, discard, diff, show, save, load, exit", parts[0]))
                } else {
                    Self::cli_error(&format!("Unknown command: {}. Available: show, configure, exit", parts[0]))
                }
            }
        }
    }

    // ── Tree navigation helpers ─────────────────────────────────

    fn get_nested<'a>(config: &'a ConfigTree, path: &[&str]) -> Option<&'a ConfigTree> {
        let mut current = config;
        for key in path {
            match current {
                serde_json::Value::Object(map) => {
                    current = map.get(*key)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    fn set_nested(config: &mut ConfigTree, path: &[&str], value: ConfigTree) -> Result<()> {
        if path.is_empty() {
            anyhow::bail!("Empty path");
        }

        let mut current = config;
        for key in &path[..path.len() - 1] {
            if let serde_json::Value::Object(map) = current {
                if !map.contains_key(*key) {
                    map.insert(key.to_string(), serde_json::json!({}));
                }
                current = map.get_mut(*key).unwrap();
            } else {
                anyhow::bail!("Cannot navigate into non-object at key '{}'", key);
            }
        }

        let last_key = path.last().unwrap();
        if let serde_json::Value::Object(map) = current {
            map.insert(last_key.to_string(), value);
        } else {
            anyhow::bail!("Parent is not an object");
        }

        Ok(())
    }

    fn delete_nested(config: &mut ConfigTree, path: &[&str]) -> Result<()> {
        if path.is_empty() {
            anyhow::bail!("Empty path");
        }

        let mut current = config;
        for key in &path[..path.len() - 1] {
            if let serde_json::Value::Object(map) = current {
                current = map.get_mut(*key).ok_or_else(|| {
                    anyhow::anyhow!("Path '{}' not found", path.join("."))
                })?;
            } else {
                anyhow::bail!("Cannot navigate into non-object");
            }
        }

        let last_key = path.last().unwrap();
        if let serde_json::Value::Object(map) = current {
            map.remove(*last_key);
        }

        Ok(())
    }

    // ── Diff engine ─────────────────────────────────────────────

    fn compute_diff(old: &ConfigTree, new: &ConfigTree, path: &str) -> Vec<ConfigDiffEntry> {
        let mut changes = Vec::new();

        match (old, new) {
            (serde_json::Value::Object(old_map), serde_json::Value::Object(new_map)) => {
                let mut all_keys: Vec<&String> = old_map.keys().chain(new_map.keys()).collect();
                all_keys.sort();
                all_keys.dedup();

                for key in all_keys {
                    let full_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    match (old_map.get(key), new_map.get(key)) {
                        (None, Some(val)) => {
                            changes.push(ConfigDiffEntry {
                                op: "set".into(),
                                path: full_path,
                                value: Some(val.clone()),
                                old: None,
                                new: None,
                            });
                        }
                        (Some(_), None) => {
                            changes.push(ConfigDiffEntry {
                                op: "delete".into(),
                                path: full_path,
                                value: None,
                                old: Some(old_map[key].clone()),
                                new: None,
                            });
                        }
                        (Some(old_val), Some(new_val)) => {
                            if old_val != new_val {
                                if old_val.is_object() && new_val.is_object() {
                                    changes.extend(Self::compute_diff(old_val, new_val, &full_path));
                                } else {
                                    changes.push(ConfigDiffEntry {
                                        op: "update".into(),
                                        path: full_path,
                                        value: None,
                                        old: Some(old_val.clone()),
                                        new: Some(new_val.clone()),
                                    });
                                }
                            }
                        }
                        (None, None) => {}
                    }
                }
            }
            _ => {
                if old != new {
                    changes.push(ConfigDiffEntry {
                        op: "update".into(),
                        path: path.to_string(),
                        value: None,
                        old: Some(old.clone()),
                        new: Some(new.clone()),
                    });
                }
            }
        }

        changes
    }

    fn format_diff(diff: &[ConfigDiffEntry]) -> String {
        if diff.is_empty() {
            return "(no differences)".into();
        }

        diff.iter()
            .map(|entry| match entry.op.as_str() {
                "set" => format!("+ {} = {}", entry.path, entry.value.as_ref().map(|v| v.to_string()).unwrap_or_default()),
                "delete" => format!("- {}", entry.path),
                "update" => format!(
                    "~ {}: {} -> {}",
                    entry.path,
                    entry.old.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                    entry.new.as_ref().map(|v| v.to_string()).unwrap_or_default()
                ),
                _ => format!("? {} ({})", entry.path, entry.op),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ── Tree formatting ─────────────────────────────────────────

    fn format_tree(config: &ConfigTree, prefix: &str, path_prefix: &str) -> String {
        let mut lines = Vec::new();

        if let serde_json::Value::Object(map) = config {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(k, _)| k.clone());

            for (i, (key, value)) in entries.iter().enumerate() {
                let is_last = i == entries.len() - 1;
                let connector = if is_last { "└── " } else { "├── " };
                let full_path = if path_prefix.is_empty() {
                    (*key).clone()
                } else {
                    format!("{}.{}", path_prefix, key)
                };

                match value {
                    serde_json::Value::Object(_) => {
                        lines.push(format!("{}{}{}", prefix, connector, key));
                        let extension = if is_last { "    " } else { "│   " };
                        lines.push(Self::format_tree(value, &format!("{}{}", prefix, extension), &full_path));
                    }
                    serde_json::Value::Array(arr) => {
                        lines.push(format!("{}{}{}", prefix, connector, key));
                        let extension = if is_last { "    " } else { "│   " };
                        if arr.is_empty() {
                            lines.push(format!("{}└── (empty)", prefix));
                        } else {
                            for (j, item) in arr.iter().enumerate() {
                                let item_last = j == arr.len() - 1;
                                let item_connector = if item_last { "└── " } else { "├── " };
                                lines.push(format!("{}{}{}{} = {}", prefix, extension, item_connector, j, item));
                            }
                        }
                    }
                    _ => {
                        lines.push(format!("{}{}{} = {}", prefix, connector, key, value));
                    }
                }
            }
        }

        lines.join("\n")
    }

    // ── Command implementations ─────────────────────────────────

    fn cmd_set(&self, args: &[&str]) -> CliResult {
        if args.is_empty() {
            return Self::cli_error("Usage: set <path...> <value>");
        }

        let (path_parts, value) = if args.len() >= 2 {
            let path = &args[..args.len() - 1];
            let val = args.last().unwrap();
            (path, Some(val.to_string()))
        } else {
            (args, None)
        };

        let mut staging = self.staging.lock().unwrap();
        let config = if let Some(ref s) = *staging {
            s.clone()
        } else {
            self.committed.lock().unwrap().clone()
        };

        let mut new_config = config;
        let parsed_value = match &value {
            Some(v) => parse_json_value(v),
            None => serde_json::json!(true),
        };

        if let Err(e) = Self::set_nested(&mut new_config, path_parts, parsed_value) {
            return Self::cli_error(&format!("Failed to set: {}", e));
        }

        if let Err(e) = self.save_staging(&new_config) {
            return Self::cli_error(&format!("Failed to save staging: {}", e));
        }

        *staging = Some(new_config);
        drop(staging);

        let path_str = path_parts.join(".");
        let val_display = value.unwrap_or_else(|| "true".into());
        Self::cli_ok_with_data(
            &format!("Set {} = {}", path_str, val_display),
            serde_json::json!({"staged": true, "path": path_str}),
        )
    }

    fn cmd_delete(&self, args: &[&str]) -> CliResult {
        if args.is_empty() {
            return Self::cli_error("Usage: delete <path...>");
        }

        let mut staging = self.staging.lock().unwrap();
        let config = if let Some(ref s) = *staging {
            s.clone()
        } else {
            self.committed.lock().unwrap().clone()
        };

        let mut new_config = config;
        if let Err(e) = Self::delete_nested(&mut new_config, args) {
            return Self::cli_error(&format!("Failed to delete: {}", e));
        }

        if let Err(e) = self.save_staging(&new_config) {
            return Self::cli_error(&format!("Failed to save staging: {}", e));
        }

        *staging = Some(new_config);
        drop(staging);

        Self::cli_ok_with_data(
            &format!("Deleted {}", args.join(".")),
            serde_json::json!({"staged": true, "path": args.join(".")}),
        )
    }

    fn cmd_commit(&self) -> CliResult {
        let staging = self.staging.lock().unwrap().clone();
        let staging = match staging {
            Some(s) => s,
            None => return Self::cli_ok("No changes to commit"),
        };

        let committed = self.committed.lock().unwrap().clone();
        let diff = Self::compute_diff(&committed, &staging, "");

        if diff.is_empty() {
            self.clear_staging();
            return Self::cli_ok("No differences to commit");
        }

        // Save history before committing
        let _ = self.save_history(&committed, "pre-commit snapshot");

        // Apply
        {
            let mut c = self.committed.lock().unwrap();
            *c = staging.clone();
        }

        if let Err(e) = self.save_committed(&staging) {
            return Self::cli_error(&format!("Failed to save committed config: {}", e));
        }

        let version = self.save_history(&staging, &format!("Committed {} change(s)", diff.len()));
        self.clear_staging();

        Self::cli_ok_with_data(
            &format!("Committed {} change(s)", diff.len()),
            serde_json::json!({
                "version": version,
                "changes": diff.len(),
                "diff": Self::format_diff(&diff),
            }),
        )
    }

    fn cmd_rollback(&self, args: &[&str]) -> CliResult {
        if args.is_empty() {
            return Self::cli_error("Usage: rollback <version>");
        }

        let version = args[0];
        let snapshot = match self.load_history_version(version) {
            Some(s) => s,
            None => return Self::cli_error(&format!("Version {} not found", version)),
        };

        // Save current to history before rollback
        let current = self.committed.lock().unwrap().clone();
        let _ = self.save_history(&current, &format!("pre-rollback to {}", version));

        // Apply rollback
        {
            let mut c = self.committed.lock().unwrap();
            *c = snapshot.config.clone();
        }

        if let Err(e) = self.save_committed(&snapshot.config) {
            return Self::cli_error(&format!("Failed to save rolled back config: {}", e));
        }

        let new_version = self.save_history(&snapshot.config, &format!("Rolled back to {}", version));
        self.clear_staging();

        Self::cli_ok_with_data(
            &format!("Rolled back to version {}", version),
            serde_json::json!({
                "new_version": new_version,
                "rolled_back_to": version,
            }),
        )
    }

    fn cmd_discard(&self) -> CliResult {
        self.clear_staging();
        *self.staging.lock().unwrap() = None;
        Self::cli_ok("Staged changes discarded")
    }

    fn cmd_diff(&self, args: &[&str]) -> CliResult {
        let (old, new) = if args.len() == 2 {
            let snap1 = self.load_history_version(args[0]);
            let snap2 = self.load_history_version(args[1]);
            match (snap1, snap2) {
                (Some(a), Some(b)) => (a.config, b.config),
                (None, _) => return Self::cli_error(&format!("Version {} not found", args[0])),
                (_, None) => return Self::cli_error(&format!("Version {} not found", args[1])),
            }
        } else {
            let committed = self.committed.lock().unwrap().clone();
            let staging = self.staging.lock().unwrap().clone();
            match staging {
                Some(s) => (committed, s),
                None => return Self::cli_ok("No staged changes to diff"),
            }
        };

        let diff = Self::compute_diff(&old, &new, "");

        Self::cli_ok_with_data(
            &format!("{} difference(s)", diff.len()),
            serde_json::json!({
                "changes": diff.len(),
                "diff": Self::format_diff(&diff),
                "entries": diff,
            }),
        )
    }

    fn cmd_show(&self, args: &[&str], session: &CliSession) -> CliResult {
        let config = self.committed.lock().unwrap().clone();

        if args.is_empty() {
            let tree = Self::format_tree(&config, "", "");
            return Self::cli_ok(&tree);
        }

        match args[0] {
            "interfaces" => {
                let path = if args.len() > 1 { &args[1..] } else { &[] };
                self.show_section(&config, "interfaces", path)
            }
            "pppoe" => self.show_section(&config, "pppoe", &[]),
            "dhcp" => self.show_section(&config, "dhcp", &[]),
            "dns" => self.show_section(&config, "dns", &[]),
            "nat" => self.show_section(&config, "nat", &[]),
            "firewall" => self.show_section(&config, "firewall", &[]),
            "ipv6" => self.show_section(&config, "ipv6", &[]),
            "vpn" => self.show_section(&config, "vpn", &[]),
            "version" | "version history" => {
                let entries = self.list_history();
                let formatted: Vec<String> = entries.iter().rev().take(20).map(|e| {
                    format!("{}  {}  {}", e.version, e.timestamp, e.message)
                }).collect();
                Self::cli_ok(&formatted.join("\n"))
            }
            "pending" => {
                let staging = self.staging.lock().unwrap().clone();
                match staging {
                    Some(s) => {
                        let committed = self.committed.lock().unwrap().clone();
                        let diff = Self::compute_diff(&committed, &s, "");
                        if diff.is_empty() {
                            Self::cli_ok("No pending changes")
                        } else {
                            Self::cli_ok(&Self::format_diff(&diff))
                        }
                    }
                    None => Self::cli_ok("No pending changes"),
                }
            }
            "templates" => {
                let templates = self.list_templates();
                let formatted: Vec<String> = templates.iter().map(|t| {
                    format!("  {} - {}", t.name, t.description)
                }).collect();
                Self::cli_ok(&format!("Saved templates:\n{}", formatted.join("\n")))
            }
            _ => Self::cli_error(&format!(
                "Unknown show target: {}. Available: interfaces, pppoe, dhcp, dns, nat, firewall, ipv6, vpn, version, pending, templates",
                args[0]
            )),
        }
    }

    fn show_section(&self, config: &ConfigTree, section: &str, subpath: &[&str]) -> CliResult {
        let mut path_parts = vec![section];
        path_parts.extend(subpath);

        match Self::get_nested(config, &path_parts) {
            Some(value) => {
                let tree = Self::format_tree(value, "  ", &path_parts.join("."));
                Self::cli_ok(&format!("{}:\n{}", section, tree))
            }
            None => Self::cli_error(&format!("Section '{}' not found", section)),
        }
    }

    fn cmd_save_template(&self, args: &[&str]) -> CliResult {
        if args.is_empty() {
            return Self::cli_error("Usage: save <template-name>");
        }

        let name = args[0];
        let description = if args.len() > 1 { args[1..].join(" ") } else { String::new() };

        let config = self.committed.lock().unwrap().clone();
        match self.save_template(name, &description, &config) {
            Ok(()) => Self::cli_ok(&format!("Template '{}' saved", name)),
            Err(e) => Self::cli_error(&format!("Failed to save template: {}", e)),
        }
    }

    fn cmd_load_template(&self, args: &[&str]) -> CliResult {
        if args.is_empty() {
            return Self::cli_error("Usage: load <template-name>");
        }

        let name = args[0];
        match self.load_template(name) {
            Some(template) => {
                let mut staging = self.staging.lock().unwrap();
                *staging = Some(template.config);
                if let Err(e) = self.save_staging(staging.as_ref().unwrap()) {
                    return Self::cli_error(&format!("Failed to save staging: {}", e));
                }
                Self::cli_ok(&format!("Template '{}' loaded to staging. Use 'commit' to apply.", name))
            }
            None => Self::cli_error(&format!("Template '{}' not found", name)),
        }
    }

    // ── History persistence ─────────────────────────────────────

    fn save_history(&self, config: &ConfigTree, message: &str) -> String {
        let _ = fs::create_dir_all(HISTORY_DIR);
        let version = config_hash(config);
        let timestamp = now_iso();

        let snapshot = ConfigVersion {
            version: version.clone(),
            timestamp: timestamp.clone(),
            message: message.to_string(),
            config: config.clone(),
        };

        let filepath = PathBuf::from(HISTORY_DIR).join(format!("{}.json", version));
        if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
            let _ = fs::write(filepath, json);
        }

        // Update index
        let index_path = PathBuf::from(HISTORY_DIR).join("index.json");
        let mut index: Vec<HistoryEntry> = fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        index.push(HistoryEntry {
            version: version.clone(),
            timestamp,
            message: message.to_string(),
        });

        // Keep last 100 entries
        if index.len() > 100 {
            index = index.split_off(index.len() - 100);
        }

        if let Ok(json) = serde_json::to_string_pretty(&index) {
            let _ = fs::write(index_path, json);
        }

        version
    }

    fn load_history_version(&self, version: &str) -> Option<ConfigVersion> {
        let filepath = PathBuf::from(HISTORY_DIR).join(format!("{}.json", version));
        let content = fs::read_to_string(filepath).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn list_history(&self) -> Vec<HistoryEntry> {
        let index_path = PathBuf::from(HISTORY_DIR).join("index.json");
        fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    // ── Template persistence ────────────────────────────────────

    fn save_template(&self, name: &str, description: &str, config: &ConfigTree) -> Result<()> {
        fs::create_dir_all(TEMPLATE_DIR).context("Failed to create template dir")?;

        let template = ConfigTemplate {
            name: name.to_string(),
            description: description.to_string(),
            created: now_iso(),
            config: config.clone(),
        };

        let filepath = PathBuf::from(TEMPLATE_DIR).join(format!("{}.json", name));
        let json = serde_json::to_string_pretty(&template)?;
        fs::write(filepath, json).context("Failed to write template")?;
        Ok(())
    }

    pub fn load_template(&self, name: &str) -> Option<ConfigTemplate> {
        let filepath = PathBuf::from(TEMPLATE_DIR).join(format!("{}.json", name));
        let content = fs::read_to_string(filepath).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn list_templates(&self) -> Vec<ConfigTemplate> {
        let _ = fs::create_dir_all(TEMPLATE_DIR);
        let mut templates = Vec::new();

        if let Ok(entries) = fs::read_dir(TEMPLATE_DIR) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(template) = serde_json::from_str::<ConfigTemplate>(&content) {
                            templates.push(template);
                        }
                    }
                }
            }
        }

        templates.sort_by(|a, b| a.name.cmp(&b.name));
        templates
    }

    // ── Public API for handlers ─────────────────────────────────

    /// Get the full configuration tree.
    pub fn get_tree(&self) -> ConfigTree {
        self.committed.lock().unwrap().clone()
    }

    /// Get the staged configuration tree.
    pub fn get_staging_tree(&self) -> Option<ConfigTree> {
        self.staging.lock().unwrap().clone()
    }

    /// Set a value in staging.
    pub fn api_set(&self, path: &str, value: ConfigTree) -> Result<CliResult> {
        let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            anyhow::bail!("Empty path");
        }

        let mut staging = self.staging.lock().unwrap();
        let config = if let Some(ref s) = *staging {
            s.clone()
        } else {
            self.committed.lock().unwrap().clone()
        };

        let mut new_config = config;
        Self::set_nested(&mut new_config, &parts, value)?;

        self.save_staging(&new_config)?;
        *staging = Some(new_config);

        Ok(Self::cli_ok_with_data(
            &format!("Set {} (staged)", path),
            serde_json::json!({"staged": true, "path": path}),
        ))
    }

    /// Delete a value in staging.
    pub fn api_delete(&self, path: &str) -> Result<CliResult> {
        let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            anyhow::bail!("Empty path");
        }

        let mut staging = self.staging.lock().unwrap();
        let config = if let Some(ref s) = *staging {
            s.clone()
        } else {
            self.committed.lock().unwrap().clone()
        };

        let mut new_config = config;
        Self::delete_nested(&mut new_config, &parts)?;

        self.save_staging(&new_config)?;
        *staging = Some(new_config);

        Ok(Self::cli_ok_with_data(
            &format!("Deleted {} (staged)", path),
            serde_json::json!({"staged": true, "path": path}),
        ))
    }

    /// Commit staged changes.
    pub fn api_commit(&self) -> Result<CliResult> {
        Ok(self.cmd_commit())
    }

    /// Rollback to a version.
    pub fn api_rollback(&self, version: &str) -> Result<CliResult> {
        Ok(self.cmd_rollback(&[version]))
    }

    /// Get diff between committed and staged.
    pub fn api_diff(&self) -> Vec<ConfigDiffEntry> {
        let committed = self.committed.lock().unwrap().clone();
        let staging = self.staging.lock().unwrap().clone();

        match staging {
            Some(s) => Self::compute_diff(&committed, &s, ""),
            None => Vec::new(),
        }
    }

    /// Get diff between two history versions.
    pub fn api_diff_versions(&self, v1: &str, v2: &str) -> Result<Vec<ConfigDiffEntry>> {
        let snap1 = self.load_history_version(v1)
            .ok_or_else(|| anyhow::anyhow!("Version {} not found", v1))?;
        let snap2 = self.load_history_version(v2)
            .ok_or_else(|| anyhow::anyhow!("Version {} not found", v2))?;
        Ok(Self::compute_diff(&snap1.config, &snap2.config, ""))
    }

    /// Apply a template with optional variable substitution.
    pub fn api_apply_template(&self, name: &str, variables: Option<&HashMap<String, String>>) -> Result<CliResult> {
        let template = self.load_template(name)
            .ok_or_else(|| anyhow::anyhow!("Template '{}' not found", name))?;

        let mut config = template.config;

        if let Some(vars) = variables {
            let config_str = serde_json::to_string(&config)?;
            let mut result = config_str;
            for (key, value) in vars {
                result = result.replace(&format!("{{{}}}", key), value);
            }
            config = serde_json::from_str(&result)?;
        }

        let mut staging = self.staging.lock().unwrap();
        self.save_staging(&config)?;
        *staging = Some(config);

        Ok(Self::cli_ok_with_data(
            &format!("Template '{}' applied to staging", name),
            serde_json::json!({"template": name, "staged": true}),
        ))
    }

    // ── Helpers ─────────────────────────────────────────────────

    fn cli_ok(message: &str) -> CliResult {
        CliResult {
            status: "ok".into(),
            message: message.to_string(),
            data: None,
            error: None,
        }
    }

    fn cli_ok_with_data(message: &str, data: serde_json::Value) -> CliResult {
        CliResult {
            status: "ok".into(),
            message: message.to_string(),
            data: Some(data),
            error: None,
        }
    }

    fn cli_error(message: &str) -> CliResult {
        CliResult {
            status: "error".into(),
            message: message.to_string(),
            data: None,
            error: Some(message.to_string()),
        }
    }
}

// ── Utility functions ────────────────────────────────────────────

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{:x}-{:x}", t.as_secs(), t.subsec_nanos())
}

fn config_hash(config: &ConfigTree) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let json = serde_json::to_string(config).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    json.hash(&mut hasher);
    format!("{:012x}", hasher.finish())
}

fn parse_json_value(s: &str) -> ConfigTree {
    // Try JSON parse first
    if let Ok(v) = serde_json::from_str::<ConfigTree>(s) {
        return v;
    }

    // Boolean strings
    match s.to_lowercase().as_str() {
        "true" | "yes" | "on" => return serde_json::json!(true),
        "false" | "no" | "off" => return serde_json::json!(false),
        _ => {}
    }

    // Integer
    if let Ok(v) = s.parse::<i64>() {
        return serde_json::json!(v);
    }

    // Float
    if let Ok(v) = s.parse::<f64>() {
        return serde_json::json!(v);
    }

    // String
    serde_json::json!(s)
}

// ── Public API functions (used by handlers) ──────────────────────

/// Get the full configuration tree.
pub fn get_tree() -> ConfigTree {
    STATE.lock().unwrap().get_tree()
}

/// Get the staged configuration tree.
pub fn get_staging_tree() -> Option<ConfigTree> {
    STATE.lock().unwrap().get_staging_tree()
}

/// Set a config value.
pub fn set_value(path: &str, value: ConfigTree) -> Result<CliResult> {
    STATE.lock().unwrap().api_set(path, value)
}

/// Delete a config value.
pub fn delete_value(path: &str) -> Result<CliResult> {
    STATE.lock().unwrap().api_delete(path)
}

/// Commit staged changes.
pub fn commit() -> Result<CliResult> {
    STATE.lock().unwrap().api_commit()
}

/// Rollback to a version.
pub fn rollback(version: &str) -> Result<CliResult> {
    STATE.lock().unwrap().api_rollback(version)
}

/// Get diff between committed and staged.
pub fn get_diff() -> Vec<ConfigDiffEntry> {
    STATE.lock().unwrap().api_diff()
}

/// Get diff between two versions.
pub fn get_diff_versions(v1: &str, v2: &str) -> Result<Vec<ConfigDiffEntry>> {
    STATE.lock().unwrap().api_diff_versions(v1, v2)
}

/// List config history.
pub fn list_history() -> Vec<HistoryEntry> {
    STATE.lock().unwrap().list_history()
}

/// List templates.
pub fn list_templates() -> Vec<ConfigTemplate> {
    STATE.lock().unwrap().list_templates()
}

/// Save a template.
pub fn save_template(name: &str, description: &str, config: &ConfigTree) -> Result<()> {
    STATE.lock().unwrap().save_template(name, description, config)
}

/// Load a template.
pub fn load_template(name: &str) -> Option<ConfigTemplate> {
    STATE.lock().unwrap().load_template(name)
}

/// Apply a template.
pub fn apply_template(name: &str, variables: Option<&HashMap<String, String>>) -> Result<CliResult> {
    STATE.lock().unwrap().api_apply_template(name, variables)
}

/// Execute a CLI command.
pub fn execute_cli(session_id: &str, command: &str) -> CliResult {
    STATE.lock().unwrap().execute_command(session_id, command)
}

/// Create a new CLI session.
pub fn create_session() -> CliSession {
    STATE.lock().unwrap().create_session()
}

/// Get a CLI session.
pub fn get_session(id: &str) -> Option<CliSession> {
    STATE.lock().unwrap().get_session(id)
}

/// Delete a CLI session.
pub fn delete_session(id: &str) {
    STATE.lock().unwrap().delete_session(id)
}

// ── Global state ─────────────────────────────────────────────────

use std::sync::LazyLock;

static STATE: LazyLock<Mutex<ConfigCliState>> = LazyLock::new(|| Mutex::new(ConfigCliState::new()));
