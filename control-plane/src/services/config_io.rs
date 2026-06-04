//! Configuration Import/Export service for router migration
//!
//! Provides export of all config as a single JSON file, import with validation,
//! and selective import (choose which sections to import).
//! Inspired by Landscape's config management approach.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tracing::info;

// ── Export format ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigExport {
    /// Export format version for future compatibility
    pub version: String,
    /// ISO 8601 timestamp of export
    pub exported_at: String,
    /// Hostname of the source router
    pub hostname: String,
    /// The configuration sections
    pub config: ConfigSections,
    /// Optional metadata about the export
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ExportMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// VectorOS version on source router
    pub vectoros_version: String,
    /// Description or notes for this export
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSections {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interfaces: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pppoe: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nat: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firewall: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traffic: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frr: Option<serde_json::Value>,
}

impl ConfigSections {
    /// Get a section by name
    pub fn get_section(&self, name: &str) -> Option<&serde_json::Value> {
        match name {
            "interfaces" => self.interfaces.as_ref(),
            "pppoe" => self.pppoe.as_ref(),
            "dhcp" => self.dhcp.as_ref(),
            "dns" => self.dns.as_ref(),
            "nat" => self.nat.as_ref(),
            "firewall" => self.firewall.as_ref(),
            "ipv6" => self.ipv6.as_ref(),
            "vpn" => self.vpn.as_ref(),
            "qos" => self.qos.as_ref(),
            "traffic" => self.traffic.as_ref(),
            "frr" => self.frr.as_ref(),
            _ => None,
        }
    }

    /// Set a section by name
    pub fn set_section(&mut self, name: &str, value: serde_json::Value) -> bool {
        match name {
            "interfaces" => { self.interfaces = Some(value); true }
            "pppoe" => { self.pppoe = Some(value); true }
            "dhcp" => { self.dhcp = Some(value); true }
            "dns" => { self.dns = Some(value); true }
            "nat" => { self.nat = Some(value); true }
            "firewall" => { self.firewall = Some(value); true }
            "ipv6" => { self.ipv6 = Some(value); true }
            "vpn" => { self.vpn = Some(value); true }
            "qos" => { self.qos = Some(value); true }
            "traffic" => { self.traffic = Some(value); true }
            "frr" => { self.frr = Some(value); true }
            _ => false,
        }
    }

    /// List all section names that have values
    pub fn section_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.interfaces.is_some() { names.push("interfaces"); }
        if self.pppoe.is_some() { names.push("pppoe"); }
        if self.dhcp.is_some() { names.push("dhcp"); }
        if self.dns.is_some() { names.push("dns"); }
        if self.nat.is_some() { names.push("nat"); }
        if self.firewall.is_some() { names.push("firewall"); }
        if self.ipv6.is_some() { names.push("ipv6"); }
        if self.vpn.is_some() { names.push("vpn"); }
        if self.qos.is_some() { names.push("qos"); }
        if self.traffic.is_some() { names.push("traffic"); }
        if self.frr.is_some() { names.push("frr"); }
        names
    }

    /// Merge another sections into this one (for selective import)
    pub fn merge_from(&mut self, other: &ConfigSections, sections: &[&str]) {
        for section in sections {
            if let Some(value) = other.get_section(section) {
                self.set_section(section, value.clone());
            }
        }
    }
}

// ── Import request ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRequest {
    /// The full export data to import
    pub export: ConfigExport,
    /// Sections to import (empty = all sections)
    #[serde(default)]
    pub sections: Vec<String>,
    /// Whether to overwrite existing values
    #[serde(default = "default_true")]
    pub overwrite: bool,
    /// Whether to commit immediately (otherwise stage only)
    #[serde(default)]
    pub auto_commit: bool,
    /// Description for the import operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_true() -> bool { true }

// ── Validation ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub sections_found: Vec<String>,
    pub import_summary: ImportSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub section: String,
    pub field: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub section: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub total_sections: usize,
    pub sections_to_import: usize,
    pub estimated_changes: usize,
}

// ── Import history ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportHistoryEntry {
    pub id: i64,
    pub imported_at: String,
    pub source_hostname: String,
    pub source_version: String,
    pub sections_imported: String,
    pub description: String,
    pub status: String,
    pub auto_commit: bool,
}

// ── Implementation ────────────────────────────────────────────

/// All valid config section names
const VALID_SECTIONS: &[&str] = &[
    "interfaces", "pppoe", "dhcp", "dns", "nat",
    "firewall", "ipv6", "vpn", "qos", "traffic", "frr",
];

/// Create a config export from the current configuration tree
pub fn export_config(hostname: Option<&str>, description: Option<&str>) -> Result<ConfigExport> {
    let tree = crate::services::config_cli::get_tree();

    let sections = extract_sections_from_tree(&tree);

    let hostname_str = hostname.unwrap_or("vectoros-router").to_string();

    let export = ConfigExport {
        version: "1.0".to_string(),
        exported_at: Utc::now().to_rfc3339(),
        hostname: hostname_str,
        config: sections,
        metadata: Some(ExportMetadata {
            vectoros_version: env!("CARGO_PKG_VERSION").to_string(),
            description: description.map(|s| s.to_string()),
            tags: Vec::new(),
        }),
    };

    info!("Config exported: {} sections", export.config.section_names().len());
    Ok(export)
}

/// Extract config sections from the config tree
fn extract_sections_from_tree(tree: &serde_json::Value) -> ConfigSections {
    ConfigSections {
        interfaces: tree.get("interfaces").cloned(),
        pppoe: tree.get("pppoe").cloned(),
        dhcp: tree.get("dhcp").cloned(),
        dns: tree.get("dns").cloned(),
        nat: tree.get("nat").cloned(),
        firewall: tree.get("firewall").cloned(),
        ipv6: tree.get("ipv6").cloned(),
        vpn: tree.get("vpn").cloned(),
        qos: tree.get("qos").cloned(),
        traffic: tree.get("traffic").cloned(),
        frr: tree.get("frr").cloned(),
    }
}

/// Validate an import request before applying
pub fn validate_import(export: &ConfigExport, sections: &[String]) -> Result<ValidationResult> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut sections_found = Vec::new();

    // Validate version
    if export.version != "1.0" {
        errors.push(ValidationError {
            section: "version".to_string(),
            field: "version".to_string(),
            message: format!("Unsupported export version: {}. Expected 1.0", export.version),
            severity: "error".to_string(),
        });
    }

    // Validate timestamp format
    if chrono::DateTime::parse_from_rfc3339(&export.exported_at).is_err() {
        warnings.push(ValidationWarning {
            section: "metadata".to_string(),
            message: "Export timestamp is not valid RFC 3339 format".to_string(),
        });
    }

    // Determine which sections to validate
    let sections_to_check: Vec<&str> = if sections.is_empty() {
        export.config.section_names()
    } else {
        sections.iter().map(|s| s.as_str()).collect()
    };

    // Validate each section
    for section_name in &sections_to_check {
        if !VALID_SECTIONS.contains(section_name) {
            errors.push(ValidationError {
                section: section_name.to_string(),
                field: "name".to_string(),
                message: format!("Unknown section: {}", section_name),
                severity: "error".to_string(),
            });
            continue;
        }

        if let Some(section_data) = export.config.get_section(section_name) {
            sections_found.push(section_name.to_string());
            validate_section(section_name, section_data, &mut errors, &mut warnings);
        } else {
            warnings.push(ValidationWarning {
                section: section_name.to_string(),
                message: format!("Section '{}' not present in export", section_name),
            });
        }
    }

    let total_sections = export.config.section_names().len();
    let sections_to_import = if sections.is_empty() {
        sections_found.len()
    } else {
        sections.len().min(sections_found.len())
    };

    let valid = errors.is_empty();

    Ok(ValidationResult {
        valid,
        errors,
        warnings,
        sections_found,
        import_summary: ImportSummary {
            total_sections,
            sections_to_import,
            estimated_changes: sections_to_import * 3, // rough estimate
        },
    })
}

/// Validate a single config section
fn validate_section(
    name: &str,
    data: &serde_json::Value,
    errors: &mut Vec<ValidationError>,
    warnings: &mut Vec<ValidationWarning>,
) {
    match name {
        "interfaces" => {
            if let Some(obj) = data.as_object() {
                for (iface_name, iface_config) in obj {
                    if let Some(config) = iface_config.as_object() {
                        // Validate MTU
                        if let Some(mtu) = config.get("mtu") {
                            if let Some(mtu_val) = mtu.as_u64() {
                                if mtu_val < 68 || mtu_val > 9000 {
                                    errors.push(ValidationError {
                                        section: "interfaces".to_string(),
                                        field: format!("{}.mtu", iface_name),
                                        message: format!("MTU {} is out of valid range (68-9000)", mtu_val),
                                        severity: "error".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        "pppoe" => {
            if let Some(obj) = data.as_object() {
                // Warn if username is empty
                if let Some(username) = obj.get("username") {
                    if username.as_str() == Some("") {
                        warnings.push(ValidationWarning {
                            section: "pppoe".to_string(),
                            message: "PPPoE username is empty".to_string(),
                        });
                    }
                }
            }
        }
        "dhcp" => {
            if let Some(obj) = data.as_object() {
                // Validate IP range
                if let (Some(start), Some(end)) = (obj.get("start_ip"), obj.get("end_ip")) {
                    if let (Some(s), Some(e)) = (start.as_str(), end.as_str()) {
                        if s == e {
                            warnings.push(ValidationWarning {
                                section: "dhcp".to_string(),
                                message: "DHCP start and end IP are the same".to_string(),
                            });
                        }
                    }
                }
            }
        }
        "dns" => {
            if let Some(obj) = data.as_object() {
                if let Some(upstream) = obj.get("upstream") {
                    if let Some(arr) = upstream.as_array() {
                        if arr.is_empty() {
                            warnings.push(ValidationWarning {
                                section: "dns".to_string(),
                                message: "No upstream DNS servers configured".to_string(),
                            });
                        }
                    }
                }
            }
        }
        "firewall" => {
            // Firewall rules validation
            if let Some(obj) = data.as_object() {
                if let Some(rules) = obj.get("rules") {
                    if let Some(arr) = rules.as_array() {
                        for (i, rule) in arr.iter().enumerate() {
                            if let Some(rule_obj) = rule.as_object() {
                                if let Some(action) = rule_obj.get("action") {
                                    if let Some(action_str) = action.as_str() {
                                        if !["accept", "drop", "reject"].contains(&action_str) {
                                            errors.push(ValidationError {
                                                section: "firewall".to_string(),
                                                field: format!("rules[{}].action", i),
                                                message: format!("Invalid firewall action: {}", action_str),
                                                severity: "error".to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {
            // Generic validation - just check it's a valid JSON object or array
            if data.is_null() {
                warnings.push(ValidationWarning {
                    section: name.to_string(),
                    message: format!("Section '{}' is null", name),
                });
            }
        }
    }
}

/// Import configuration from an export
pub fn import_config(
    request: &ImportRequest,
    auto_commit: bool,
) -> Result<serde_json::Value> {
    let export = &request.export;
    let sections_to_import: Vec<&str> = if request.sections.is_empty() {
        export.config.section_names()
    } else {
        request.sections.iter().map(|s| s.as_str()).collect()
    };

    let mut applied_sections = Vec::new();
    let mut errors = Vec::new();

    for section_name in &sections_to_import {
        match import_section(section_name, &export.config, request.overwrite) {
            Ok(()) => {
                applied_sections.push(section_name.to_string());
            }
            Err(e) => {
                errors.push(format!("Section '{}': {}", section_name, e));
            }
        }
    }

    let mut result = serde_json::json!({
        "status": if errors.is_empty() { "ok" } else { "partial" },
        "sections_imported": applied_sections,
        "errors": errors,
        "auto_commit": auto_commit,
    });

    // Auto-commit if requested and no errors
    if auto_commit && errors.is_empty() && !applied_sections.is_empty() {
        match crate::services::config_cli::commit() {
            Ok(cli_result) => {
                if let serde_json::Value::Object(mut map) = result {
                    map.insert("commit_result".to_string(), serde_json::json!(cli_result));
                    result = serde_json::Value::Object(map);
                }
            }
            Err(e) => {
                if let serde_json::Value::Object(mut map) = result {
                    map.insert("commit_error".to_string(), serde_json::json!(e.to_string()));
                    result = serde_json::Value::Object(map);
                }
            }
        }
    }

    // Record import in history
    let _ = record_import(export, &request.sections, auto_commit, &applied_sections);

    info!("Config imported: {} sections applied", applied_sections.len());
    Ok(result)
}

/// Import a single section into the config tree
fn import_section(
    section_name: &str,
    sections: &ConfigSections,
    overwrite: bool,
) -> Result<()> {
    let section_data = sections.get_section(section_name)
        .ok_or_else(|| anyhow::anyhow!("Section '{}' not found in export", section_name))?;

    if overwrite {
        // Set the entire section
        set_section_tree(section_name, section_data.clone())?;
    } else {
        // Merge into existing section
        let current_tree = crate::services::config_cli::get_tree();
        if let Some(current_section) = current_tree.get(section_name) {
            let merged = merge_json_values(current_section, section_data);
            set_section_tree(section_name, merged)?;
        } else {
            set_section_tree(section_name, section_data.clone())?;
        }
    }

    Ok(())
}

/// Set a section in the config tree
fn set_section_tree(section_name: &str, value: serde_json::Value) -> Result<()> {
    crate::services::config_cli::set_value(section_name, value)
        .map_err(|e| anyhow::anyhow!("Failed to set section '{}': {}", section_name, e))?;
    Ok(())
}

/// Merge two JSON values recursively (new values override old)
fn merge_json_values(base: &serde_json::Value, override_val: &serde_json::Value) -> serde_json::Value {
    match (base, override_val) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(override_map)) => {
            let mut result = base_map.clone();
            for (key, value) in override_map {
                if let Some(base_value) = result.get(key) {
                    if base_value.is_object() && value.is_object() {
                        result.insert(key.clone(), merge_json_values(base_value, value));
                    } else {
                        result.insert(key.clone(), value.clone());
                    }
                } else {
                    result.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        _ => override_val.clone(),
    }
}

/// Record an import in the history database
fn record_import(
    export: &ConfigExport,
    sections: &[String],
    auto_commit: bool,
    applied: &[String],
) -> Result<()> {
    let db = crate::db::get();

    let sections_str = if sections.is_empty() {
        applied.join(",")
    } else {
        sections.join(",")
    };

    let source_version = export.metadata.as_ref()
        .map(|m| m.vectoros_version.clone())
        .unwrap_or_default();

    let description = export.metadata.as_ref()
        .and_then(|m| m.description.clone())
        .unwrap_or_else(|| "Config import".to_string());

    db.record_import(
        &export.hostname,
        &source_version,
        &sections_str,
        &description,
        auto_commit,
    )?;

    Ok(())
}

/// Get import history
pub fn get_import_history(limit: Option<i64>) -> Result<Vec<ImportHistoryEntry>> {
    let db = crate::db::get();
    let entries = db.get_import_history(limit.unwrap_or(50))?;
    Ok(entries)
}

/// Export config as a downloadable JSON string
pub fn export_as_json(hostname: Option<&str>, description: Option<&str>) -> Result<String> {
    let export = export_config(hostname, description)?;
    let json = serde_json::to_string_pretty(&export)
        .context("Failed to serialize config export")?;
    Ok(json)
}

/// Export config as a downloadable TOML string
pub fn export_as_toml(hostname: Option<&str>, description: Option<&str>) -> Result<String> {
    let export = export_config(hostname, description)?;
    // Convert to TOML via JSON intermediate
    let json_value = serde_json::to_value(&export)?;
    let toml = toml::to_string_pretty(&json_value)
        .context("Failed to serialize config export as TOML")?;
    Ok(toml)
}

// ── Database integration ──────────────────────────────────────

/// Add import history methods to Database
impl crate::db::Database {
    /// Record a config import
    pub fn record_import(
        &self,
        source_hostname: &str,
        source_version: &str,
        sections: &str,
        description: &str,
        auto_commit: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create table if not exists
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS config_imports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                imported_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                source_hostname TEXT NOT NULL,
                source_version TEXT NOT NULL DEFAULT '',
                sections_imported TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'completed',
                auto_commit INTEGER NOT NULL DEFAULT 0
            );
        ")?;

        conn.execute(
            "INSERT INTO config_imports (source_hostname, source_version, sections_imported, description, status, auto_commit)
             VALUES (?1, ?2, ?3, ?4, 'completed', ?5)",
            params![source_hostname, source_version, sections, description, auto_commit as i32],
        )?;

        Ok(())
    }

    /// Get import history
    pub fn get_import_history(&self, limit: i64) -> Result<Vec<ImportHistoryEntry>> {
        let conn = self.conn.lock().unwrap();

        // Ensure table exists
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS config_imports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                imported_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                source_hostname TEXT NOT NULL,
                source_version TEXT NOT NULL DEFAULT '',
                sections_imported TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'completed',
                auto_commit INTEGER NOT NULL DEFAULT 0
            );
        ")?;

        let mut stmt = conn.prepare(
            "SELECT id, imported_at, source_hostname, source_version, sections_imported, description, status, auto_commit
             FROM config_imports
             ORDER BY id DESC
             LIMIT ?1"
        )?;

        let entries = stmt.query_map(params![limit], |row| {
            Ok(ImportHistoryEntry {
                id: row.get(0)?,
                imported_at: row.get(1)?,
                source_hostname: row.get(2)?,
                source_version: row.get(3)?,
                sections_imported: row.get(4)?,
                description: row.get(5)?,
                status: row.get(6)?,
                auto_commit: row.get::<_, i32>(7)? != 0,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }
}

// ── Public API functions (used by handlers) ──────────────────

/// Export config as JSON string
pub fn api_export_json(hostname: Option<&str>, description: Option<&str>) -> Result<String> {
    export_as_json(hostname, description)
}

/// Validate an import
pub fn api_validate_import(export_json: &str, sections: &[String]) -> Result<ValidationResult> {
    let export: ConfigExport = serde_json::from_str(export_json)
        .context("Failed to parse config export JSON")?;
    validate_import(&export, sections)
}

/// Import config from JSON string
pub fn api_import_config(
    import_json: &str,
    sections: Vec<String>,
    overwrite: bool,
    auto_commit: bool,
) -> Result<serde_json::Value> {
    let request: ImportRequest = serde_json::from_str(import_json)
        .context("Failed to parse import request")?;

    let mut effective_request = request;
    if !sections.is_empty() {
        effective_request.sections = sections;
    }
    effective_request.overwrite = overwrite;
    effective_request.auto_commit = auto_commit;

    import_config(&effective_request, auto_commit)
}

/// Get import history
pub fn api_import_history(limit: Option<i64>) -> Result<Vec<ImportHistoryEntry>> {
    get_import_history(limit)
}
