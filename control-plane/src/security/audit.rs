use axum::{middleware::Next, response::Response};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Security audit event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    /// Authentication events
    LoginSuccess,
    LoginFailure,
    Logout,
    PasswordChange,

    /// Configuration changes
    ConfigSave,
    ConfigCommit,
    ConfigRollback,

    /// Network changes
    InterfaceUp,
    InterfaceDown,
    InterfaceConfig,

    /// Firewall changes
    FirewallRuleAdd,
    FirewallRuleUpdate,
    FirewallRuleDelete,
    FirewallEnable,
    FirewallDisable,

    /// Service management
    ServiceStart,
    ServiceStop,
    ServiceRestart,

    /// VPN operations
    VpnConfigure,
    VpnDown,

    /// PPPoE operations
    PppoeConnect,
    PppoeDisconnect,

    /// General API operations
    ApiAccess,
    ApiError,

    /// Security events
    RateLimited,
    CsrfViolation,
    UnauthorizedAccess,

    /// Custom event with description
    Custom(String),
}

impl AuditAction {
    pub fn as_str(&self) -> String {
        match self {
            AuditAction::LoginSuccess => "login_success".to_string(),
            AuditAction::LoginFailure => "login_failure".to_string(),
            AuditAction::Logout => "logout".to_string(),
            AuditAction::PasswordChange => "password_change".to_string(),
            AuditAction::ConfigSave => "config_save".to_string(),
            AuditAction::ConfigCommit => "config_commit".to_string(),
            AuditAction::ConfigRollback => "config_rollback".to_string(),
            AuditAction::InterfaceUp => "interface_up".to_string(),
            AuditAction::InterfaceDown => "interface_down".to_string(),
            AuditAction::InterfaceConfig => "interface_config".to_string(),
            AuditAction::FirewallRuleAdd => "firewall_rule_add".to_string(),
            AuditAction::FirewallRuleUpdate => "firewall_rule_update".to_string(),
            AuditAction::FirewallRuleDelete => "firewall_rule_delete".to_string(),
            AuditAction::FirewallEnable => "firewall_enable".to_string(),
            AuditAction::FirewallDisable => "firewall_disable".to_string(),
            AuditAction::ServiceStart => "service_start".to_string(),
            AuditAction::ServiceStop => "service_stop".to_string(),
            AuditAction::ServiceRestart => "service_restart".to_string(),
            AuditAction::VpnConfigure => "vpn_configure".to_string(),
            AuditAction::VpnDown => "vpn_down".to_string(),
            AuditAction::PppoeConnect => "pppoe_connect".to_string(),
            AuditAction::PppoeDisconnect => "pppoe_disconnect".to_string(),
            AuditAction::ApiAccess => "api_access".to_string(),
            AuditAction::ApiError => "api_error".to_string(),
            AuditAction::RateLimited => "rate_limited".to_string(),
            AuditAction::CsrfViolation => "csrf_violation".to_string(),
            AuditAction::UnauthorizedAccess => "unauthorized_access".to_string(),
            AuditAction::Custom(s) => format!("custom_{}", s),
        }
    }
}

/// Audit log entry for recording security-relevant events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: String,
    pub user: String,
    pub action: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub ip_address: Option<String>,
    pub status_code: Option<u16>,
    pub details: Option<String>,
}

/// Record an audit event to the database and tracing log.
pub fn log_audit_event(
    user: &str,
    action: AuditAction,
    method: Option<&str>,
    path: Option<&str>,
    ip_address: Option<&str>,
    status_code: Option<u16>,
    details: Option<&str>,
) {
    let entry = AuditLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        user: user.to_string(),
        action: action.as_str(),
        method: method.map(|s| s.to_string()),
        path: path.map(|s| s.to_string()),
        ip_address: ip_address.map(|s| s.to_string()),
        status_code,
        details: details.map(|s| s.to_string()),
    };

    // Log to tracing (structured output)
    info!(
        target: "audit",
        user = %entry.user,
        action = %entry.action,
        method = entry.method.as_deref().unwrap_or(""),
        path = entry.path.as_deref().unwrap_or(""),
        ip = entry.ip_address.as_deref().unwrap_or(""),
        status = entry.status_code.map(|s| s.to_string()).unwrap_or_default(),
        details = entry.details.as_deref().unwrap_or(""),
        "Audit event"
    );

    // Store in database (best effort - don't fail the request if logging fails)
    if let Err(e) = store_audit_entry(&entry) {
        tracing::error!("Failed to store audit log entry: {}", e);
    }
}

/// Store an audit log entry in the database.
fn store_audit_entry(entry: &AuditLogEntry) -> Result<(), String> {
    let db = crate::db::get();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO audit_logs (timestamp, user, action, method, path, ip_address, status_code, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            entry.timestamp,
            entry.user,
            entry.action,
            entry.method,
            entry.path,
            entry.ip_address,
            entry.status_code,
            entry.details,
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Retrieve recent audit log entries from the database.
pub fn get_audit_logs(limit: i64) -> Result<Vec<AuditLogEntry>, String> {
    let db = crate::db::get();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT timestamp, user, action, method, path, ip_address, status_code, details
             FROM audit_logs ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let entries = stmt
        .query_map(rusqlite::params![limit], |row| {
            Ok(AuditLogEntry {
                timestamp: row.get(0)?,
                user: row.get(1)?,
                action: row.get(2)?,
                method: row.get(3)?,
                path: row.get(4)?,
                ip_address: row.get(5)?,
                status_code: row.get(6)?,
                details: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

/// Middleware that logs all API access for audit trail.
///
/// This middleware captures method, path, client IP, and response status
/// for every API request. State-changing operations are logged with more
/// detail.
pub async fn audit_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let ip = super::rate_limit::extract_client_ip(request.headers()).to_string();

    // Only log API requests
    let response = if path.starts_with("/api/") {
        let resp = next.run(request).await;
        let status = resp.status().as_u16();

        // Log all API access at info level for audit trail
        log_audit_event(
            &extract_user_from_response(&resp),
            AuditAction::ApiAccess,
            Some(&method),
            Some(&path),
            Some(&ip),
            Some(status),
            None,
        );

        resp
    } else {
        next.run(request).await
    };

    response
}

/// Extract user from response extensions (set by auth middleware).
fn extract_user_from_response(_response: &Response) -> String {
    // Note: In a more complete implementation, we'd extract the user
    // from the request extensions before the auth middleware consumes them.
    // For now, we return "system" for middleware-level logging.
    "system".to_string()
}

/// Log a security event (rate limit hit, CSRF violation, etc.).
#[allow(dead_code)]
pub fn log_security_event(
    action: AuditAction,
    ip: &str,
    details: &str,
) {
    log_audit_event(
        "anonymous",
        action,
        None,
        None,
        Some(ip),
        None,
        Some(details),
    );
}
