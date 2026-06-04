//! Log management service
//!
//! Reads, filters, and manages logs from VPP, dnsmasq, and VectorOS
//! control plane. Supports level filtering, keyword search, and log rotation.
//! No Python dependency.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::info;

const LOG_DIR: &str = "/var/log/vectoros";
const MAX_LOG_SIZE_MB: u64 = 50;
const LOG_RETENTION_DAYS: u64 = 7;

/// Log sources and their file paths
fn log_sources() -> std::collections::HashMap<&'static str, &'static str> {
    let mut map = std::collections::HashMap::new();
    map.insert("vpp", "/var/log/vpp/vpp.log");
    map.insert("dnsmasq", "/var/log/dnsmasq.log");
    map.insert("vectoros", "/var/log/vectoros/control-plane.log");
    map
}

/// A parsed log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub source: String,
}

/// Query parameters for log retrieval
#[derive(Debug, Clone, Deserialize)]
pub struct LogQuery {
    pub sources: Option<String>,
    pub level: Option<String>,
    pub lines: Option<u32>,
    pub filter: Option<String>,
    pub limit: Option<u32>,
}

/// Level priority for filtering
fn level_priority(level: &str) -> u32 {
    match level.to_lowercase().as_str() {
        "debug" => 0,
        "info" => 1,
        "warn" | "warning" => 2,
        "error" => 3,
        _ => 0,
    }
}

/// Ensure the log directory exists.
fn ensure_log_dir() -> Result<()> {
    fs::create_dir_all(LOG_DIR).context("Failed to create log directory")?;
    Ok(())
}

/// Parse a log line into structured components.
fn parse_log_line(line: &str, source: &str) -> Option<LogEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Try common format: 2026-06-04 12:34:56 [level] message
    // Or: 2026-06-04T12:34:56 level message
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() >= 3 {
        let first = words[0];
        // Check if first token looks like a timestamp
        if first.len() >= 10 && (first.contains('-') || first.contains('T')) {
            // Find the level token
            for (i, word) in words[1..].iter().enumerate() {
                let normalized = word.trim_matches(|c| c == '[' || c == ']').to_lowercase();
                if matches!(
                    normalized.as_str(),
                    "debug" | "info" | "warn" | "warning" | "error"
                ) {
                    let level = if normalized == "warning" {
                        "warn"
                    } else {
                        &normalized
                    };
                    let msg_start = i + 2; // +1 for offset, +1 for the level token itself
                    let message = words[msg_start..].join(" ");
                    return Some(LogEntry {
                        timestamp: first.to_string(),
                        level: level.to_string(),
                        message,
                        source: source.to_string(),
                    });
                }
            }
        }
    }

    // Try "level: message" format
    if words.len() >= 2 {
        let normalized = words[0].trim_matches(|c| c == '[' || c == ']').to_lowercase();
        if matches!(
            normalized.as_str(),
            "debug" | "info" | "warn" | "warning" | "error"
        ) {
            let level = if normalized == "warning" {
                "warn"
            } else {
                &normalized
            };
            let message = words[1..].join(" ");
            return Some(LogEntry {
                timestamp: chrono_now(),
                level: level.to_string(),
                message,
                source: source.to_string(),
            });
        }
    }

    // Unrecognized line, treat as info
    Some(LogEntry {
        timestamp: chrono_now(),
        level: "info".to_string(),
        message: line.to_string(),
        source: source.to_string(),
    })
}

/// Read the last N lines from a log file.
fn read_log_file(filepath: &str, tail_lines: usize) -> Vec<LogEntry> {
    let path = Path::new(filepath);
    if !path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let all_lines: Vec<&str> = content.lines().collect();
    let start = if all_lines.len() > tail_lines {
        all_lines.len() - tail_lines
    } else {
        0
    };

    all_lines[start..]
        .iter()
        .filter_map(|line| parse_log_line(line, ""))
        .collect()
}

/// Show logs from specified sources with optional filtering.
pub fn show(query: LogQuery) -> Result<serde_json::Value> {
    ensure_log_dir()?;

    let sources = log_sources();
    let source_names: Vec<&str> = query
        .sources
        .as_deref()
        .map(|s| s.split(',').map(|s| s.trim()).collect())
        .unwrap_or_else(|| sources.keys().copied().collect());

    let min_level = query.level.as_deref().unwrap_or("debug");
    let min_priority = level_priority(min_level);
    let tail = query.lines.unwrap_or(200) as usize;
    let keyword = query.filter.as_ref().map(|s| s.to_lowercase());
    let limit = query.limit.unwrap_or(100) as usize;

    let mut all_logs: Vec<LogEntry> = Vec::new();

    for source_name in &source_names {
        if let Some(filepath) = sources.get(source_name) {
            let mut entries = read_log_file(filepath, tail);
            for entry in &mut entries {
                entry.source = source_name.to_string();
            }
            all_logs.extend(entries);
        }
    }

    // Filter by level
    all_logs.retain(|e| level_priority(&e.level) >= min_priority);

    // Filter by keyword
    if let Some(ref kw) = keyword {
        all_logs.retain(|e| e.message.to_lowercase().contains(kw));
    }

    // Sort by timestamp descending
    all_logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Limit output
    all_logs.truncate(limit);

    Ok(serde_json::json!({
        "status": "ok",
        "count": all_logs.len(),
        "logs": all_logs,
    }))
}

/// Clear logs for specified sources.
pub fn clear(sources: Option<&str>) -> Result<serde_json::Value> {
    ensure_log_dir()?;

    let all_sources = log_sources();
    let source_names: Vec<&str> = sources
        .map(|s| s.split(',').map(|s| s.trim()).collect())
        .unwrap_or_else(|| all_sources.keys().copied().collect());

    let mut results = Vec::new();

    for source_name in &source_names {
        if let Some(filepath) = all_sources.get(source_name) {
            let path = Path::new(filepath);
            if path.exists() {
                let timestamp = chrono_now().replace([':', ' '], "");
                let rotated_name = format!("{}.{}.bak", filepath, timestamp);
                match fs::rename(path, &rotated_name) {
                    Ok(_) => {
                        // Create new empty log file
                        let _ = fs::write(filepath, "");
                        results.push(serde_json::json!({
                            "source": source_name,
                            "status": "ok",
                            "message": format!("Log cleared, rotated to {}", rotated_name),
                        }));
                    }
                    Err(e) => {
                        results.push(serde_json::json!({
                            "source": source_name,
                            "status": "error",
                            "message": e.to_string(),
                        }));
                    }
                }
            } else {
                // Create empty file
                let _ = fs::write(filepath, "");
                results.push(serde_json::json!({
                    "source": source_name,
                    "status": "ok",
                    "message": "Log file created (was empty)",
                }));
            }
        } else {
            results.push(serde_json::json!({
                "source": source_name,
                "status": "error",
                "message": "Unknown source",
            }));
        }
    }

    Ok(serde_json::json!({
        "status": "ok",
        "results": results,
    }))
}

/// Rotate log files that exceed the size threshold.
pub fn rotate(sources: Option<&str>, force: bool) -> Result<serde_json::Value> {
    ensure_log_dir()?;

    let all_sources = log_sources();
    let source_names: Vec<&str> = sources
        .map(|s| s.split(',').map(|s| s.trim()).collect())
        .unwrap_or_else(|| all_sources.keys().copied().collect());

    let mut results = Vec::new();

    for source_name in &source_names {
        if let Some(filepath) = all_sources.get(source_name) {
            let path = Path::new(filepath);
            if !path.exists() {
                results.push(serde_json::json!({
                    "source": source_name,
                    "status": "ok",
                    "message": "No log file to rotate",
                }));
                continue;
            }

            let size_mb = fs::metadata(path)
                .map(|m| m.len() / (1024 * 1024))
                .unwrap_or(0);

            if size_mb < MAX_LOG_SIZE_MB && !force {
                results.push(serde_json::json!({
                    "source": source_name,
                    "status": "ok",
                    "message": format!("Log is {}MB, below threshold ({}MB)", size_mb, MAX_LOG_SIZE_MB),
                }));
                continue;
            }

            let timestamp = chrono_now().replace([':', ' '], "");
            let rotated_name = format!("{}.{}.bak", filepath, timestamp);
            match fs::rename(path, &rotated_name) {
                Ok(_) => {
                    let _ = fs::write(filepath, "");
                    results.push(serde_json::json!({
                        "source": source_name,
                        "status": "ok",
                        "message": format!("Rotated to {}", rotated_name),
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "source": source_name,
                        "status": "error",
                        "message": e.to_string(),
                    }));
                }
            }
        }
    }

    // Clean old rotated logs
    let cleaned = cleanup_old_backups()?;

    Ok(serde_json::json!({
        "status": "ok",
        "results": results,
        "cleaned_old": cleaned,
    }))
}

/// Remove rotated log backups older than LOG_RETENTION_DAYS.
fn cleanup_old_backups() -> Result<u32> {
    let log_dir = Path::new(LOG_DIR);
    if !log_dir.exists() {
        return Ok(0);
    }

    let cutoff = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        - (LOG_RETENTION_DAYS * 86400);

    let mut cleaned = 0u32;

    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".bak") {
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                                if duration.as_secs() < cutoff {
                                    let _ = fs::remove_file(&path);
                                    cleaned += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(cleaned)
}

/// Simple timestamp without requiring the chrono crate.
fn chrono_now() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let mut year = 1970i64;
    let mut day_of_year = days as i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if day_of_year < days_in_year {
            break;
        }
        day_of_year -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    let mut day = day_of_year as u32 + 1;
    for (i, &md) in month_days.iter().enumerate() {
        if day <= md {
            month = (i + 1) as u32;
            break;
        }
        day -= md;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
