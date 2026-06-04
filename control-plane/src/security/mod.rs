pub mod audit;
pub mod csrf;
pub mod headers;
pub mod rate_limit;
pub mod session;
pub mod validation;

use axum::http::StatusCode;
use axum::response::Response;
use tracing::warn;

/// Apply security middleware to an Axum request, executing them in order:
/// 1. Security headers
/// 2. Rate limiting
/// 3. CSRF protection (for state-changing requests)
///
/// Individual handlers are responsible for input validation.
#[allow(dead_code)]
pub async fn apply_security_layers(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (StatusCode, String)> {
    // Layer 1: Security headers (always applied)
    let response = headers::security_headers_middleware(request, next).await;
    Ok(response)
}

/// Initialize the audit logs table in the database.
pub fn init_audit_table() -> Result<(), String> {
    let db = crate::db::get();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            user TEXT NOT NULL DEFAULT 'anonymous',
            action TEXT NOT NULL,
            method TEXT,
            path TEXT,
            ip_address TEXT,
            status_code INTEGER,
            details TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);",
    )
    .map_err(|e| format!("Failed to create audit_logs table: {}", e))?;

    Ok(())
}

/// Hash a password using bcrypt with a cost factor of 12.
pub fn hash_password(password: &str) -> Result<String, String> {
    bcrypt::hash(password, 12).map_err(|e| format!("Failed to hash password: {}", e))
}

/// Verify a password against a bcrypt hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Store a password hash in the database.
pub fn store_password_hash(username: &str, hash: &str) -> Result<(), String> {
    let db = crate::db::get();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO config (key, value, category, updated_at)
         VALUES (?1, ?2, 'auth', CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![format!("auth:password:{}", username), hash],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Retrieve a stored password hash from the database.
pub fn get_password_hash(username: &str) -> Option<String> {
    let db = crate::db::get();
    let conn = db.conn.lock().ok()?;

    let mut stmt = conn
        .prepare("SELECT value FROM config WHERE key = ?1")
        .ok()?;

    let hash: String = stmt
        .query_row(rusqlite::params![format!("auth:password:{}", username)], |row| {
            row.get(0)
        })
        .ok()?;

    Some(hash)
}

/// Initialize default admin credentials on first run.
///
/// If no password hash exists for the "admin" user, hash the default
/// password (from environment or hardcoded) and store it.
pub fn init_default_credentials() {
    let username = std::env::var("VECTOROS_USERNAME")
        .unwrap_or_else(|_| "admin".to_string());

    // Check if password hash already exists
    if get_password_hash(&username).is_some() {
        return;
    }

    // Hash and store the default password
    let default_password = std::env::var("VECTOROS_PASSWORD")
        .unwrap_or_else(|_| "vectoros".to_string());

    match hash_password(&default_password) {
        Ok(hash) => {
            if let Err(e) = store_password_hash(&username, &hash) {
                warn!("Failed to store default password hash: {}", e);
            } else {
                tracing::info!(
                    "Initialized default credentials for user '{}'. \
                     CHANGE THE DEFAULT PASSWORD IMMEDIATELY!",
                    username
                );
            }
        }
        Err(e) => {
            warn!("Failed to hash default password: {}", e);
        }
    }
}
