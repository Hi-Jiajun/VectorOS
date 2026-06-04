use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

/// VectorOS database for configuration storage
/// Inspired by Landscape's SQLite-based config management

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub id: i64,
    pub key: String,
    pub value: String,
    pub category: String,
    pub updated_at: String,
}

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    /// Open or create the database
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Enable WAL mode for better concurrent access and performance
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=-8000;
            PRAGMA mmap_size=268435456;
            PRAGMA temp_store=MEMORY;
            PRAGMA busy_timeout=5000;
        ")?;

        // Create tables
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS config (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL UNIQUE,
                value TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS config_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                category TEXT NOT NULL,
                version INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS interfaces (
                name TEXT PRIMARY KEY,
                config TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'down',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS pppoe_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                interface TEXT NOT NULL,
                username TEXT NOT NULL,
                password TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'disconnected',
                session_id INTEGER,
                ip_address TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS firewall_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                action TEXT NOT NULL,
                protocol TEXT,
                src_ip TEXT,
                dst_ip TEXT,
                src_port TEXT,
                dst_port TEXT,
                enabled INTEGER DEFAULT 1,
                order_num INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS dhcp_leases (
                mac TEXT PRIMARY KEY,
                ip TEXT NOT NULL,
                hostname TEXT,
                interface TEXT,
                expires_at DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS vpn_connections (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                vpn_type TEXT NOT NULL,
                config TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'disconnected',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS system_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                level TEXT NOT NULL,
                source TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            -- Performance indexes
            CREATE INDEX IF NOT EXISTS idx_config_category ON config(category);
            CREATE INDEX IF NOT EXISTS idx_config_history_key ON config_history(key, version DESC);
            CREATE INDEX IF NOT EXISTS idx_config_history_version ON config_history(version DESC);
            CREATE INDEX IF NOT EXISTS idx_pppoe_sessions_status ON pppoe_sessions(status);
            CREATE INDEX IF NOT EXISTS idx_firewall_rules_enabled ON firewall_rules(enabled, order_num);
            CREATE INDEX IF NOT EXISTS idx_firewall_rules_action ON firewall_rules(action);
            CREATE INDEX IF NOT EXISTS idx_dhcp_leases_interface ON dhcp_leases(interface);
            CREATE INDEX IF NOT EXISTS idx_dhcp_leases_expires ON dhcp_leases(expires_at);
            CREATE INDEX IF NOT EXISTS idx_vpn_connections_type ON vpn_connections(vpn_type, status);
            CREATE INDEX IF NOT EXISTS idx_system_logs_level ON system_logs(level);
            CREATE INDEX IF NOT EXISTS idx_system_logs_source ON system_logs(source);
            CREATE INDEX IF NOT EXISTS idx_system_logs_created ON system_logs(created_at DESC);
        ")?;

        info!("Database initialized at {}", path);
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Get a config value
    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM config WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Set a config value
    pub fn set_config(&self, key: &str, value: &str, category: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Save to history first
        conn.execute(
            "INSERT INTO config_history (key, value, category, version)
             SELECT key, value, category, COALESCE(MAX(version), 0) + 1
             FROM config_history WHERE key = ?1
             UNION ALL
             SELECT ?1, ?2, ?3, 1
             WHERE NOT EXISTS (SELECT 1 FROM config_history WHERE key = ?1)",
            params![key, value, category],
        )?;

        // Update current value
        conn.execute(
            "INSERT INTO config (key, value, category, updated_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                category = excluded.category,
                updated_at = CURRENT_TIMESTAMP",
            params![key, value, category],
        )?;

        Ok(())
    }

    /// Get all config entries
    pub fn get_all_config(&self) -> Result<Vec<ConfigEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, key, value, category, updated_at FROM config ORDER BY category, key")?;
        let entries: Vec<ConfigEntry> = stmt.query_map([], |row| {
            Ok(ConfigEntry {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                category: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }

    /// Get config history for a key
    pub fn get_config_history(&self, key: &str) -> Result<Vec<ConfigEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, key, value, category, created_at FROM config_history
             WHERE key = ?1 ORDER BY version DESC LIMIT 50"
        )?;
        let entries: Vec<ConfigEntry> = stmt.query_map(params![key], |row| {
            Ok(ConfigEntry {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                category: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }

    /// Save interface config
    pub fn save_interface(&self, name: &str, config: &str, state: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO interfaces (name, config, state, updated_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(name) DO UPDATE SET
                config = excluded.config,
                state = excluded.state,
                updated_at = CURRENT_TIMESTAMP",
            params![name, config, state],
        )?;
        Ok(())
    }

    /// Get all interfaces
    pub fn get_interfaces(&self) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name, config, state FROM interfaces ORDER BY name")?;
        let interfaces: Vec<(String, String, String)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<Result<Vec<_>>>()?;
        Ok(interfaces)
    }

    /// Add system log
    pub fn add_log(&self, level: &str, source: &str, message: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO system_logs (level, source, message) VALUES (?1, ?2, ?3)",
            params![level, source, message],
        )?;
        Ok(())
    }

    /// Get recent logs
    pub fn get_logs(&self, limit: i64) -> Result<Vec<(String, String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT level, source, message, created_at FROM system_logs
             ORDER BY id DESC LIMIT ?1"
        )?;
        let logs: Vec<(String, String, String, String)> = stmt.query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?.collect::<Result<Vec<_>>>()?;
        Ok(logs)
    }
}

/// Global database instance
static DB: std::sync::OnceLock<Database> = std::sync::OnceLock::new();

pub fn init(path: &str) -> Result<()> {
    let db = Database::open(path)?;
    DB.set(db).map_err(|_| rusqlite::Error::ExecuteReturnedResults)?;
    Ok(())
}

pub fn get() -> &'static Database {
    DB.get().expect("Database not initialized")
}
