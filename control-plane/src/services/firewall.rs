//! Firewall management service — OPNsense-style
//!
//! Provides rule groups, aliases (IP/port/network/URL), schedule-based rules,
//! GeoIP blocking, rule reordering, traffic-shaper integration, and
//! Suricata IDS management.  All state is persisted to a single JSON file
//! and applied to VPP via vppctl ACL commands.

use anyhow::{Context, Result};
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;
use utoipa::ToSchema;

const RULES_FILE: &str = "/etc/vectoros/firewall-rules.json";
const SURICATA_CONF: &str = "/etc/suricata/suricata.yaml";

// ── Top-level data model ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallData {
    pub enabled: bool,
    #[serde(default)]
    pub default_policy: String, // "block" | "pass"
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
    #[serde(default)]
    pub groups: Vec<RuleGroup>,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    #[serde(default)]
    pub geoip: GeoIpConfig,
    #[serde(default)]
    pub shaper: ShaperConfig,
    #[serde(default)]
    pub ids: IdsConfig,
}

impl Default for FirewallData {
    fn default() -> Self {
        Self {
            enabled: true,
            default_policy: "block".into(),
            rules: Vec::new(),
            groups: Vec::new(),
            aliases: Vec::new(),
            schedules: Vec::new(),
            geoip: GeoIpConfig::default(),
            shaper: ShaperConfig::default(),
            ids: IdsConfig::default(),
        }
    }
}

// ── Firewall Rule ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub id: u32,
    pub action: String,           // "pass" | "block" | "reject"
    pub enabled: bool,
    pub direction: String,        // "in" | "out" | "both"
    pub protocol: Option<String>, // "tcp" | "udp" | "icmp" | "ip" | …
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_port: Option<String>, // port, range, or alias name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_alias: Option<String>,   // alias name for source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_alias: Option<String>,   // alias name for destination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_port_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_port_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,       // rule group name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,    // schedule name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_rule_order")]
    pub order: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dscp: Option<String>,        // DSCP marking: "ef", "af11", …
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_prefix: Option<String>,
    #[serde(default)]
    pub geoip_countries: Vec<String>, // ISO country codes to match
    #[serde(default)]
    pub match_group_geoip: bool,      // use GeoIP action instead of rule action
}

fn default_rule_order() -> u32 {
    0
}

// ── Rule Group ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGroup {
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rules: Vec<u32>, // rule IDs in order
    #[serde(default)]
    pub interfaces: Vec<String>,
}

// ── Alias ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    #[serde(rename = "type")]
    pub alias_type: String, // "host" | "network" | "port" | "url"
    pub description: Option<String>,
    pub enabled: bool,
    /// For host/network: list of IPs/CIDRs.  For port: list of port numbers/ranges.
    /// For url: list of URLs to fetch lists from.
    #[serde(default)]
    pub entries: Vec<String>,
    /// For url aliases: cached entries fetched from URLs.
    #[serde(default)]
    pub cached_entries: Vec<String>,
    /// When the URL entries were last fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fetched: Option<String>,
    /// Auto-refresh interval in seconds for URL aliases (0 = manual).
    #[serde(default)]
    pub refresh_interval: u64,
}

// ── Schedule ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    /// Time ranges per weekday (0=Sunday .. 6=Saturday).
    /// Each entry: { "day": 0-6, "start": "HH:MM", "end": "HH:MM" }
    pub time_ranges: Vec<TimeRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeRange {
    pub day: u8,       // 0=Sunday .. 6=Saturday
    pub start: String, // "HH:MM"
    pub end: String,   // "HH:MM"
}

impl Schedule {
    /// Returns true if the schedule is currently active.
    pub fn is_active_now(&self) -> bool {
        if !self.enabled {
            return false;
        }
        let now = chrono::Local::now();
        let day_num = now.format("%w").to_string().parse::<u8>().unwrap_or(0);
        let current_time = now.time();

        for tr in &self.time_ranges {
            if tr.day == day_num {
                if let (Ok(start), Ok(end)) = (
                    NaiveTime::parse_from_str(&tr.start, "%H:%M"),
                    NaiveTime::parse_from_str(&tr.end, "%H:%M"),
                ) {
                    if start <= end {
                        if current_time >= start && current_time <= end {
                            return true;
                        }
                    } else {
                        // overnight range, e.g. 22:00 - 06:00
                        if current_time >= start || current_time <= end {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

// ── GeoIP Config ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoIpConfig {
    pub enabled: bool,
    /// Default action when a country matches: "block" | "pass"
    #[serde(default = "default_geoip_action")]
    pub default_action: String,
    /// Country codes that are always blocked.
    #[serde(default)]
    pub blocked_countries: Vec<String>,
    /// Country codes that are always allowed.
    #[serde(default)]
    pub allowed_countries: Vec<String>,
    /// Path to the MaxMind GeoLite2-Country database.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
}

fn default_geoip_action() -> String {
    "block".into()
}

// ── Traffic Shaper (OPNsense-style) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShaperConfig {
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: HashMap<String, ShaperInterface>,
    #[serde(default)]
    pub queues: Vec<ShaperQueue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaperInterface {
    pub bandwidth: u64,       // bits/sec
    pub enabled: bool,
    #[serde(default)]
    pub download: Option<u64>,
    #[serde(default)]
    pub upload: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaperQueue {
    pub name: String,
    pub weight: u32,
    pub priority: u32,
    pub dscp: Option<String>,
    pub interface: Option<String>,
    pub description: Option<String>,
}

// ── IDS / Suricata Config ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdsConfig {
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub rule_categories: HashMap<String, bool>,
    #[serde(default)]
    pub alerts: Vec<IdsAlert>,
    #[serde(default)]
    pub stats: IdsStats,
}

impl Default for IdsConfig {
    fn default() -> Self {
        let mut categories = HashMap::new();
        categories.insert("attack".into(), true);
        categories.insert("scan".into(), true);
        categories.insert("exploit".into(), true);
        categories.insert("malware".into(), true);
        categories.insert("policy".into(), false);
        categories.insert("info".into(), false);

        Self {
            enabled: false,
            interfaces: Vec::new(),
            rule_categories: categories,
            alerts: Vec::new(),
            stats: IdsStats::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdsAlert {
    pub timestamp: String,
    pub severity: String, // "critical" | "high" | "medium" | "low" | "info"
    pub category: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: Option<u32>,
    pub dst_port: Option<u32>,
    pub protocol: String,
    pub signature: String,
    pub description: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdsStats {
    pub packets_inspected: u64,
    pub alerts_total: u64,
    pub alerts_blocked: u64,
    pub uptime_seconds: u64,
    pub rules_loaded: u32,
}

// ── Request types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRuleRequest {
    pub action: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    #[serde(default)]
    pub src_port: Option<String>,
    #[serde(default)]
    pub dst_port: Option<String>,
    #[serde(default)]
    pub src_alias: Option<String>,
    #[serde(default)]
    pub dst_alias: Option<String>,
    #[serde(default)]
    pub src_port_alias: Option<String>,
    #[serde(default)]
    pub dst_port_alias: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub log: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub log_prefix: Option<String>,
    #[serde(default)]
    pub geoip_countries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuleRequest {
    pub id: u32,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    #[serde(default)]
    pub src_port: Option<String>,
    #[serde(default)]
    pub dst_port: Option<String>,
    #[serde(default)]
    pub src_alias: Option<String>,
    #[serde(default)]
    pub dst_alias: Option<String>,
    #[serde(default)]
    pub src_port_alias: Option<String>,
    #[serde(default)]
    pub dst_port_alias: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub log: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub geoip_countries: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderRequest {
    pub rule_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAliasRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub alias_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub refresh_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddScheduleRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub time_ranges: Vec<TimeRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdsConfigRequest {
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub rule_categories: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaperIfaceRequest {
    pub interface: String,
    pub bandwidth: u64,
    #[serde(default)]
    pub download: Option<u64>,
    #[serde(default)]
    pub upload: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaperQueueRequest {
    pub name: String,
    pub weight: u32,
    pub priority: u32,
    #[serde(default)]
    pub dscp: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

// ── Persistence ──────────────────────────────────────────────────────

fn load_rules() -> FirewallData {
    let path = Path::new(RULES_FILE);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str::<FirewallData>(&content) {
                return data;
            }
        }
    }
    FirewallData::default()
}

fn save_rules(data: &FirewallData) -> Result<()> {
    let path = Path::new(RULES_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create firewall directory")?;
    }
    let json = serde_json::to_string_pretty(data).context("Failed to serialize firewall data")?;
    fs::write(path, json).context("Failed to write firewall data")?;
    Ok(())
}

// ── VPP helpers ──────────────────────────────────────────────────────

pub fn run_vppctl(args: &[&str]) -> Result<String> {
    let output = Command::new("vppctl")
        .args(args)
        .output()
        .context("Failed to execute vppctl")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("vppctl failed: {}", stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolve an alias to its concrete entries (flattening nested aliases).
fn resolve_alias<'a>(name: &str, aliases: &'a [Alias]) -> Vec<&'a str> {
    for a in aliases {
        if a.name == name && a.enabled {
            let mut result = Vec::new();
            for entry in &a.entries {
                // Check for nested alias references
                let mut is_nested = false;
                for inner in aliases {
                    if inner.name == *entry && inner.enabled {
                        is_nested = true;
                        for inner_entry in resolve_alias(&inner.name, aliases) {
                            result.push(inner_entry);
                        }
                    }
                }
                if !is_nested {
                    result.push(entry.as_str());
                }
            }
            return result;
        }
    }
    Vec::new()
}

/// Apply the full rule set to VPP via vppctl ACL commands.
fn apply_rules_to_vpp(data: &FirewallData) -> Result<Vec<serde_json::Value>> {
    let _ = run_vppctl(&["acl", "plugin", "enable"]);

    let mut results = Vec::new();

    // Sort rules by order field
    let mut sorted_rules: Vec<&FirewallRule> = data.rules.iter().filter(|r| r.enabled).collect();
    sorted_rules.sort_by_key(|r| r.order);

    for rule in sorted_rules {
        // Check schedule
        if let Some(ref schedule_name) = rule.schedule {
            if let Some(schedule) = data.schedules.iter().find(|s| &s.name == schedule_name) {
                if !schedule.is_active_now() {
                    results.push(serde_json::json!({
                        "rule_id": rule.id,
                        "skipped": true,
                        "reason": "schedule not active",
                    }));
                    continue;
                }
            }
        }

        // Resolve aliases
        let mut src_ips = Vec::new();
        let mut dst_ips = Vec::new();
        let mut src_ports = Vec::new();
        let mut dst_ports = Vec::new();

        if let Some(ref src_alias) = rule.src_alias {
            src_ips = resolve_alias(src_alias, &data.aliases)
                .into_iter()
                .map(String::from)
                .collect();
        }
        if let Some(ref dst_alias) = rule.dst_alias {
            dst_ips = resolve_alias(dst_alias, &data.aliases)
                .into_iter()
                .map(String::from)
                .collect();
        }
        if let Some(ref src_port_alias) = rule.src_port_alias {
            src_ports = resolve_alias(src_port_alias, &data.aliases)
                .into_iter()
                .map(String::from)
                .collect();
        }
        if let Some(ref dst_port_alias) = rule.dst_port_alias {
            dst_ports = resolve_alias(dst_port_alias, &data.aliases)
                .into_iter()
                .map(String::from)
                .collect();
        }

        // Fall back to literal values if no alias
        if src_ips.is_empty() {
            if let Some(ref ip) = rule.src_ip {
                src_ips.push(ip.clone());
            }
        }
        if dst_ips.is_empty() {
            if let Some(ref ip) = rule.dst_ip {
                dst_ips.push(ip.clone());
            }
        }
        if src_ports.is_empty() {
            if let Some(ref port) = rule.src_port {
                src_ports.push(port.clone());
            }
        }
        if dst_ports.is_empty() {
            if let Some(ref port) = rule.dst_port {
                dst_ports.push(port.clone());
            }
        }

        // If nothing to match, skip
        if src_ips.is_empty()
            && dst_ips.is_empty()
            && src_ports.is_empty()
            && dst_ports.is_empty()
        {
            // Default allow/block all
            let action = if rule.action == "pass" {
                "permit"
            } else {
                "deny"
            };
            let cmd = vec!["acl", "add", "action", action];
            let args: Vec<&str> = cmd.iter().map(|s| *s).collect();
            match run_vppctl(&args) {
                Ok(out) => results.push(serde_json::json!({
                    "rule_id": rule.id,
                    "entry": cmd.join(" "),
                    "stdout": out,
                    "rc": 0,
                })),
                Err(e) => results.push(serde_json::json!({
                    "rule_id": rule.id,
                    "entry": cmd.join(" "),
                    "stderr": e.to_string(),
                    "rc": 1,
                })),
            }
            continue;
        }

        // Build ACL entries — cross-product of IPs x ports
        for src_ip in &src_ips {
            for dst_ip in &dst_ips {
                for src_port in if src_ports.is_empty() {
                    vec![String::new()]
                } else {
                    src_ports.clone()
                } {
                    for dst_port in if dst_ports.is_empty() {
                        vec![String::new()]
                    } else {
                        dst_ports.clone()
                    } {
                        let action = if rule.action == "pass" {
                            "permit"
                        } else {
                            "deny"
                        };
                        let mut cmd_parts = vec![
                            "acl".to_string(),
                            "add".to_string(),
                            format!("src-ip {}", src_ip),
                            format!("dst-ip {}", dst_ip),
                        ];
                        if !src_port.is_empty() {
                            cmd_parts.push(format!("src-port {}", src_port));
                        }
                        if !dst_port.is_empty() {
                            cmd_parts.push(format!("dst-port {}", dst_port));
                        }
                        if let Some(ref proto) = rule.protocol {
                            if proto != "ip" {
                                cmd_parts.push(format!("proto {}", proto));
                            }
                        }
                        cmd_parts.push(format!("action {}", action));

                        let args: Vec<&str> = cmd_parts.iter().map(|s| s.as_str()).collect();
                        match run_vppctl(&args) {
                            Ok(out) => results.push(serde_json::json!({
                                "rule_id": rule.id,
                                "entry": cmd_parts.join(" "),
                                "stdout": out,
                                "rc": 0,
                            })),
                            Err(e) => results.push(serde_json::json!({
                                "rule_id": rule.id,
                                "entry": cmd_parts.join(" "),
                                "stderr": e.to_string(),
                                "rc": 1,
                            })),
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

// ── Public API ───────────────────────────────────────────────────────

// ── Rules ────────────────────────────────────────────────────────────

/// Add a firewall rule.
pub fn add_rule(req: AddRuleRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let next_id = data.rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    let next_order = data.rules.iter().map(|r| r.order).max().unwrap_or(0) + 1;

    let new_rule = FirewallRule {
        id: next_id,
        action: req.action,
        enabled: true,
        direction: if req.direction.is_empty() {
            "both".into()
        } else {
            req.direction
        },
        protocol: req.protocol.or_else(|| Some("ip".into())),
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        src_alias: req.src_alias,
        dst_alias: req.dst_alias,
        src_port_alias: req.src_port_alias,
        dst_port_alias: req.dst_port_alias,
        group: req.group,
        schedule: req.schedule,
        log: Some(req.log.unwrap_or(false)),
        description: req.description,
        order: next_order,
        dscp: req.dscp,
        log_prefix: req.log_prefix,
        geoip_countries: req.geoip_countries,
        match_group_geoip: false,
    };

    data.rules.push(new_rule.clone());
    save_rules(&data)?;

    if data.enabled {
        apply_rules_to_vpp(&data)?;
    }

    info!("Firewall rule {} added, total: {}", new_rule.id, data.rules.len());
    Ok(serde_json::json!({
        "status": "ok",
        "rule": new_rule,
        "total_rules": data.rules.len(),
    }))
}

/// Update an existing firewall rule.
pub fn update_rule(req: UpdateRuleRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let rule = data
        .rules
        .iter_mut()
        .find(|r| r.id == req.id)
        .context(format!("Rule {} not found", req.id))?;

    if let Some(action) = req.action {
        rule.action = action;
    }
    if let Some(enabled) = req.enabled {
        rule.enabled = enabled;
    }
    if let Some(direction) = req.direction {
        rule.direction = direction;
    }
    if let Some(src_ip) = req.src_ip {
        rule.src_ip = Some(src_ip);
    }
    if let Some(dst_ip) = req.dst_ip {
        rule.dst_ip = Some(dst_ip);
    }
    if let Some(src_port) = req.src_port {
        rule.src_port = Some(src_port);
    }
    if let Some(dst_port) = req.dst_port {
        rule.dst_port = Some(dst_port);
    }
    if let Some(src_alias) = req.src_alias {
        rule.src_alias = Some(src_alias);
    }
    if let Some(dst_alias) = req.dst_alias {
        rule.dst_alias = Some(dst_alias);
    }
    if let Some(src_port_alias) = req.src_port_alias {
        rule.src_port_alias = Some(src_port_alias);
    }
    if let Some(dst_port_alias) = req.dst_port_alias {
        rule.dst_port_alias = Some(dst_port_alias);
    }
    if let Some(protocol) = req.protocol {
        rule.protocol = Some(protocol);
    }
    if let Some(group) = req.group {
        rule.group = Some(group);
    }
    if let Some(schedule) = req.schedule {
        rule.schedule = Some(schedule);
    }
    if let Some(log) = req.log {
        rule.log = Some(log);
    }
    if let Some(description) = req.description {
        rule.description = Some(description);
    }
    if let Some(dscp) = req.dscp {
        rule.dscp = Some(dscp);
    }
    if let Some(geoip) = req.geoip_countries {
        rule.geoip_countries = geoip;
    }

    let updated_rule = rule.clone();
    save_rules(&data)?;

    if data.enabled {
        apply_rules_to_vpp(&data)?;
    }

    info!("Firewall rule {} updated", req.id);
    Ok(serde_json::json!({
        "status": "ok",
        "rule": updated_rule,
    }))
}

/// Delete a firewall rule by ID.
pub fn del_rule(id: u32) -> Result<serde_json::Value> {
    let mut data = load_rules();
    let original_count = data.rules.len();

    data.rules.retain(|r| r.id != id);

    if data.rules.len() == original_count {
        anyhow::bail!("Rule with id {} not found", id);
    }

    // Also remove from groups
    for group in &mut data.groups {
        group.rules.retain(|&r_id| r_id != id);
    }

    save_rules(&data)?;

    if data.enabled {
        apply_rules_to_vpp(&data)?;
    }

    info!("Firewall rule {} deleted, total: {}", id, data.rules.len());
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Rule {} deleted", id),
        "total_rules": data.rules.len(),
    }))
}

/// Reorder rules.
pub fn reorder_rules(req: ReorderRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    for (idx, rule_id) in req.rule_ids.iter().enumerate() {
        if let Some(rule) = data.rules.iter_mut().find(|r| r.id == *rule_id) {
            rule.order = idx as u32;
        }
    }

    save_rules(&data)?;

    if data.enabled {
        apply_rules_to_vpp(&data)?;
    }

    Ok(serde_json::json!({
        "status": "ok",
        "message": "Rules reordered",
    }))
}

/// Show current firewall state.
pub fn show() -> Result<serde_json::Value> {
    let data = load_rules();
    let active_rules = data.rules.iter().filter(|r| r.enabled).count();

    let vpp_acl_status = run_vppctl(&["show", "acl"]).unwrap_or_else(|_| "N/A".to_string());

    // Evaluate schedule status for each rule
    let mut rules_with_status: Vec<serde_json::Value> = Vec::new();
    for rule in &data.rules {
        let schedule_active = if let Some(ref sched_name) = rule.schedule {
            data.schedules
                .iter()
                .find(|s| &s.name == sched_name)
                .map(|s| s.is_active_now())
                .unwrap_or(false)
        } else {
            true // no schedule means always active
        };

        rules_with_status.push(serde_json::json!({
            "id": rule.id,
            "action": rule.action,
            "enabled": rule.enabled,
            "direction": rule.direction,
            "protocol": rule.protocol,
            "src_ip": rule.src_ip,
            "dst_ip": rule.dst_ip,
            "src_port": rule.src_port,
            "dst_port": rule.dst_port,
            "src_alias": rule.src_alias,
            "dst_alias": rule.dst_alias,
            "src_port_alias": rule.src_port_alias,
            "dst_port_alias": rule.dst_port_alias,
            "group": rule.group,
            "schedule": rule.schedule,
            "schedule_active": schedule_active,
            "log": rule.log,
            "description": rule.description,
            "order": rule.order,
            "dscp": rule.dscp,
            "log_prefix": rule.log_prefix,
            "geoip_countries": rule.geoip_countries,
        }));
    }

    Ok(serde_json::json!({
        "status": "ok",
        "enabled": data.enabled,
        "default_policy": data.default_policy,
        "rules": rules_with_status,
        "groups": data.groups,
        "aliases": data.aliases,
        "schedules": data.schedules,
        "geoip": data.geoip,
        "shaper": data.shaper,
        "ids": {
            "enabled": data.ids.enabled,
            "interfaces": data.ids.interfaces,
            "rule_categories": data.ids.rule_categories,
            "stats": data.ids.stats,
            "recent_alerts": data.ids.alerts.iter().rev().take(50).collect::<Vec<_>>(),
        },
        "total_rules": data.rules.len(),
        "active_rules": active_rules,
        "vpp_acl_status": vpp_acl_status,
    }))
}

/// Enable the firewall.
pub fn enable() -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.enabled = true;
    save_rules(&data)?;

    let _ = run_vppctl(&["acl", "plugin", "enable"]);
    apply_rules_to_vpp(&data)?;

    info!("Firewall enabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "Firewall enabled"
    }))
}

/// Disable the firewall.
pub fn disable() -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.enabled = false;
    save_rules(&data)?;

    let _ = run_vppctl(&["acl", "plugin", "disable"]);

    info!("Firewall disabled");
    Ok(serde_json::json!({
        "status": "ok",
        "message": "Firewall disabled"
    }))
}

// ── Rule Groups ──────────────────────────────────────────────────────

/// Add a rule group.
pub fn add_group(req: AddGroupRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    if data.groups.iter().any(|g| g.name == req.name) {
        anyhow::bail!("Group '{}' already exists", req.name);
    }

    let group = RuleGroup {
        name: req.name.clone(),
        description: req.description,
        enabled: req.enabled,
        rules: Vec::new(),
        interfaces: req.interfaces,
    };

    data.groups.push(group.clone());
    save_rules(&data)?;

    info!("Rule group '{}' added", req.name);
    Ok(serde_json::json!({
        "status": "ok",
        "group": group,
    }))
}

/// Delete a rule group.
pub fn del_group(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();
    let original_count = data.groups.len();

    data.groups.retain(|g| g.name != name);

    if data.groups.len() == original_count {
        anyhow::bail!("Group '{}' not found", name);
    }

    save_rules(&data)?;

    info!("Rule group '{}' deleted", name);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Group '{}' deleted", name),
    }))
}

/// Add a rule to a group.
pub fn add_rule_to_group(group_name: &str, rule_id: u32) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let group = data
        .groups
        .iter_mut()
        .find(|g| g.name == group_name)
        .context(format!("Group '{}' not found", group_name))?;

    if !data.rules.iter().any(|r| r.id == rule_id) {
        anyhow::bail!("Rule {} not found", rule_id);
    }

    if !group.rules.contains(&rule_id) {
        group.rules.push(rule_id);
    }

    let updated_group = group.clone();
    save_rules(&data)?;

    Ok(serde_json::json!({
        "status": "ok",
        "group": updated_group,
    }))
}

/// Remove a rule from a group.
pub fn remove_rule_from_group(group_name: &str, rule_id: u32) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let group = data
        .groups
        .iter_mut()
        .find(|g| g.name == group_name)
        .context(format!("Group '{}' not found", group_name))?;

    group.rules.retain(|&r| r != rule_id);

    let updated_group = group.clone();
    save_rules(&data)?;

    Ok(serde_json::json!({
        "status": "ok",
        "group": updated_group,
    }))
}

/// List all groups.
pub fn list_groups() -> Result<serde_json::Value> {
    let data = load_rules();
    Ok(serde_json::json!({
        "status": "ok",
        "groups": data.groups,
    }))
}

// ── Aliases ──────────────────────────────────────────────────────────

/// Add an alias.
pub fn add_alias(req: AddAliasRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    if data.aliases.iter().any(|a| a.name == req.name) {
        anyhow::bail!("Alias '{}' already exists", req.name);
    }

    let alias = Alias {
        name: req.name.clone(),
        alias_type: req.alias_type,
        description: req.description,
        enabled: req.enabled,
        entries: req.entries,
        cached_entries: Vec::new(),
        last_fetched: None,
        refresh_interval: req.refresh_interval,
    };

    data.aliases.push(alias.clone());
    save_rules(&data)?;

    info!("Alias '{}' added", req.name);
    Ok(serde_json::json!({
        "status": "ok",
        "alias": alias,
    }))
}

/// Update an alias.
pub fn update_alias(
    name: &str,
    entries: Option<Vec<String>>,
    enabled: Option<bool>,
    description: Option<String>,
) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let alias = data
        .aliases
        .iter_mut()
        .find(|a| a.name == name)
        .context(format!("Alias '{}' not found", name))?;

    if let Some(e) = entries {
        alias.entries = e;
    }
    if let Some(en) = enabled {
        alias.enabled = en;
    }
    if let Some(d) = description {
        alias.description = Some(d);
    }

    let updated_alias = alias.clone();
    save_rules(&data)?;

    info!("Alias '{}' updated", name);
    Ok(serde_json::json!({
        "status": "ok",
        "alias": updated_alias,
    }))
}

/// Delete an alias.
pub fn del_alias(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();
    let original = data.aliases.len();

    data.aliases.retain(|a| a.name != name);

    if data.aliases.len() == original {
        anyhow::bail!("Alias '{}' not found", name);
    }

    // Also remove alias references from rules
    for rule in &mut data.rules {
        if rule.src_alias.as_deref() == Some(name) {
            rule.src_alias = None;
        }
        if rule.dst_alias.as_deref() == Some(name) {
            rule.dst_alias = None;
        }
        if rule.src_port_alias.as_deref() == Some(name) {
            rule.src_port_alias = None;
        }
        if rule.dst_port_alias.as_deref() == Some(name) {
            rule.dst_port_alias = None;
        }
    }

    save_rules(&data)?;

    info!("Alias '{}' deleted", name);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Alias '{}' deleted", name),
    }))
}

/// List all aliases.
pub fn list_aliases() -> Result<serde_json::Value> {
    let data = load_rules();
    Ok(serde_json::json!({
        "status": "ok",
        "aliases": data.aliases,
    }))
}

/// Refresh URL alias entries (fetch from configured URLs).
pub fn refresh_url_alias(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();

    let alias = data
        .aliases
        .iter_mut()
        .find(|a| a.name == name && a.alias_type == "url")
        .context(format!("URL alias '{}' not found", name))?;

    let mut all_entries = Vec::new();

    for url in &alias.entries {
        match Command::new("curl")
            .args(["-sL", "--max-time", "10", url])
            .output()
        {
            Ok(output) => {
                let body = String::from_utf8_lossy(&output.stdout);
                for line in body.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        all_entries.push(trimmed.to_string());
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch URL alias from {}: {}", url, e);
            }
        }
    }

    let now = chrono::Local::now().to_rfc3339();
    alias.cached_entries = all_entries;
    alias.last_fetched = Some(now);

    let refreshed_count = alias.cached_entries.len();
    let updated_alias = alias.clone();
    save_rules(&data)?;

    info!("URL alias '{}' refreshed ({} entries)", name, refreshed_count);
    Ok(serde_json::json!({
        "status": "ok",
        "alias": updated_alias,
    }))
}

// ── Schedules ────────────────────────────────────────────────────────

/// Add a schedule.
pub fn add_schedule(req: AddScheduleRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    if data.schedules.iter().any(|s| s.name == req.name) {
        anyhow::bail!("Schedule '{}' already exists", req.name);
    }

    let schedule = Schedule {
        name: req.name.clone(),
        description: req.description,
        enabled: req.enabled,
        time_ranges: req.time_ranges,
    };

    data.schedules.push(schedule.clone());
    save_rules(&data)?;

    info!("Schedule '{}' added", req.name);
    Ok(serde_json::json!({
        "status": "ok",
        "schedule": schedule,
    }))
}

/// Delete a schedule.
pub fn del_schedule(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();
    let original = data.schedules.len();

    data.schedules.retain(|s| s.name != name);

    if data.schedules.len() == original {
        anyhow::bail!("Schedule '{}' not found", name);
    }

    // Remove schedule references from rules
    for rule in &mut data.rules {
        if rule.schedule.as_deref() == Some(name) {
            rule.schedule = None;
        }
    }

    save_rules(&data)?;

    info!("Schedule '{}' deleted", name);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("Schedule '{}' deleted", name),
    }))
}

/// List all schedules.
pub fn list_schedules() -> Result<serde_json::Value> {
    let data = load_rules();
    Ok(serde_json::json!({
        "status": "ok",
        "schedules": data.schedules,
    }))
}

// ── GeoIP ────────────────────────────────────────────────────────────

/// Update GeoIP configuration.
pub fn update_geoip(config: GeoIpConfig) -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.geoip = config;
    save_rules(&data)?;

    info!("GeoIP config updated");
    Ok(serde_json::json!({
        "status": "ok",
        "geoip": data.geoip,
    }))
}

// ── Traffic Shaper ───────────────────────────────────────────────────

/// Set shaper interface bandwidth.
pub fn set_shaper_interface(req: ShaperIfaceRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.shaper.enabled = true;

    let iface = ShaperInterface {
        bandwidth: req.bandwidth,
        enabled: true,
        download: req.download,
        upload: req.upload,
    };

    data.shaper.interfaces.insert(req.interface.clone(), iface);
    save_rules(&data)?;

    // Apply via VPP policer
    let _ = run_vppctl(&[
        "set",
        "interface",
        "tx",
        "limit",
        &req.interface,
        &req.bandwidth.to_string(),
    ]);

    info!("Shaper interface '{}' set to {} bps", req.interface, req.bandwidth);
    Ok(serde_json::json!({
        "status": "ok",
        "shaper": data.shaper,
    }))
}

/// Remove shaper interface.
pub fn remove_shaper_interface(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.shaper.interfaces.remove(name);
    save_rules(&data)?;

    info!("Shaper interface '{}' removed", name);
    Ok(serde_json::json!({
        "status": "ok",
        "shaper": data.shaper,
    }))
}

/// Add a shaper queue.
pub fn add_shaper_queue(req: ShaperQueueRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();

    // Remove existing queue with same name
    data.shaper.queues.retain(|q| q.name != req.name);

    let queue = ShaperQueue {
        name: req.name.clone(),
        weight: req.weight,
        priority: req.priority,
        dscp: req.dscp,
        interface: req.interface,
        description: req.description,
    };

    data.shaper.queues.push(queue.clone());
    save_rules(&data)?;

    info!("Shaper queue '{}' added", req.name);
    Ok(serde_json::json!({
        "status": "ok",
        "queue": queue,
        "shaper": data.shaper,
    }))
}

/// Delete a shaper queue.
pub fn del_shaper_queue(name: &str) -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.shaper.queues.retain(|q| q.name != name);
    save_rules(&data)?;

    info!("Shaper queue '{}' deleted", name);
    Ok(serde_json::json!({
        "status": "ok",
        "shaper": data.shaper,
    }))
}

/// Get shaper status.
pub fn get_shaper_status() -> Result<serde_json::Value> {
    let data = load_rules();
    Ok(serde_json::json!({
        "status": "ok",
        "shaper": data.shaper,
    }))
}

// ── IDS / Suricata ───────────────────────────────────────────────────

/// Update IDS configuration.
pub fn update_ids(config: IdsConfigRequest) -> Result<serde_json::Value> {
    let mut data = load_rules();
    data.ids.enabled = config.enabled;
    data.ids.interfaces = config.interfaces;
    if let Some(cats) = config.rule_categories {
        data.ids.rule_categories = cats;
    }
    save_rules(&data)?;

    if config.enabled {
        // Try to start suricata
        let _ = Command::new("suricata")
            .args(["-c", SURICATA_CONF, "-D"])
            .output();
        info!("IDS/Suricata enabled on interfaces: {:?}", data.ids.interfaces);
    } else {
        // Try to stop suricata
        let _ = Command::new("killall").arg("suricata").output();
        info!("IDS/Suricata disabled");
    }

    Ok(serde_json::json!({
        "status": "ok",
        "ids": {
            "enabled": data.ids.enabled,
            "interfaces": data.ids.interfaces,
            "rule_categories": data.ids.rule_categories,
        },
    }))
}

/// Get IDS alerts.
pub fn get_ids_alerts(limit: Option<u32>) -> Result<serde_json::Value> {
    let data = load_rules();
    let limit = limit.unwrap_or(100) as usize;

    let alerts: Vec<&IdsAlert> = data.ids.alerts.iter().rev().take(limit).collect();

    Ok(serde_json::json!({
        "status": "ok",
        "alerts": alerts,
        "total": data.ids.alerts.len(),
        "stats": data.ids.stats,
    }))
}

/// Clear IDS alerts.
pub fn clear_ids_alerts() -> Result<serde_json::Value> {
    let mut data = load_rules();
    let count = data.ids.alerts.len();
    data.ids.alerts.clear();
    save_rules(&data)?;

    info!("{} IDS alerts cleared", count);
    Ok(serde_json::json!({
        "status": "ok",
        "message": format!("{} alerts cleared", count),
    }))
}

/// Get IDS stats.
pub fn get_ids_stats() -> Result<serde_json::Value> {
    let data = load_rules();
    Ok(serde_json::json!({
        "status": "ok",
        "stats": data.ids.stats,
    }))
}
