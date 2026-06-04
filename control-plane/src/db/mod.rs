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

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

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
        let rows = stmt.query_map([], |row| {
            Ok(ConfigEntry {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                category: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Get config history for a key
    pub fn get_config_history(&self, key: &str) -> Result<Vec<ConfigEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, key, value, category, created_at FROM config_history
             WHERE key = ?1 ORDER BY version DESC LIMIT 50"
        )?;
        let rows = stmt.query_map(params![key], |row| {
            Ok(ConfigEntry {
                id: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                category: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
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
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let mut interfaces = Vec::new();
        for row in rows {
            interfaces.push(row?);
        }
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
        let rows = stmt.query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }
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
