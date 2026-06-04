use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Session data for an authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub session_id: String,
    /// Username associated with this session.
    pub username: String,
    /// Client IP address at time of creation.
    pub client_ip: String,
    /// User agent string.
    pub user_agent: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
    /// Last time this session was used.
    pub last_active: DateTime<Utc>,
    /// CSRF token for this session.
    pub csrf_token: String,
}

impl Session {
    /// Check if the session is still valid (not expired).
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    /// Check if the session matches the given IP and user agent.
    pub fn matches_client(&self, ip: &str, user_agent: &str) -> bool {
        self.client_ip == ip && self.user_agent == user_agent
    }

    /// Update the last active timestamp.
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }
}

/// Configuration for session management.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Session duration in hours (default: 24).
    pub session_duration_hours: i64,
    /// Maximum number of concurrent sessions per user (default: 5).
    pub max_sessions_per_user: usize,
    /// Whether to enforce IP binding (default: false).
    pub enforce_ip_binding: bool,
    /// Whether to enforce user agent binding (default: false).
    pub enforce_user_agent: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_duration_hours: 24,
            max_sessions_per_user: 5,
            enforce_ip_binding: false,
            enforce_user_agent: false,
        }
    }
}

/// Session manager that tracks active sessions.
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    config: SessionConfig,
}

impl SessionManager {
    pub fn new(config: SessionConfig) -> Self {
        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        };

        // Spawn cleanup task
        let cleanup_sessions = manager.sessions.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                Self::cleanup_expired(&cleanup_sessions).await;
            }
        });

        manager
    }

    /// Create a new session for a user.
    pub async fn create_session(
        &self,
        username: &str,
        client_ip: &str,
        user_agent: &str,
        csrf_token: &str,
    ) -> Session {
        let session_id = super::csrf::generate_random_token();
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.config.session_duration_hours);

        let session = Session {
            session_id: session_id.clone(),
            username: username.to_string(),
            client_ip: client_ip.to_string(),
            user_agent: user_agent.to_string(),
            created_at: now,
            expires_at,
            last_active: now,
            csrf_token: csrf_token.to_string(),
        };

        let mut sessions = self.sessions.write().await;

        // Remove old sessions for this user if over limit
        let user_sessions: Vec<String> = sessions
            .values()
            .filter(|s| s.username == username)
            .map(|s| s.session_id.clone())
            .collect();

        if user_sessions.len() >= self.config.max_sessions_per_user {
            // Remove oldest session
            if let Some(oldest) = user_sessions.first() {
                sessions.remove(oldest);
                info!("Removed oldest session for user '{}'", username);
            }
        }

        sessions.insert(session_id.clone(), session.clone());
        info!("Created session {} for user '{}'", session_id, username);

        session
    }

    /// Validate a session.
    pub async fn validate_session(
        &self,
        session_id: &str,
        client_ip: &str,
        user_agent: &str,
    ) -> Option<Session> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            if !session.is_valid() {
                info!("Session {} expired for user '{}'", session_id, session.username);
                sessions.remove(session_id);
                return None;
            }

            // Check IP binding if enforced
            if self.config.enforce_ip_binding && session.client_ip != client_ip {
                warn!(
                    "Session {} IP mismatch for user '{}': expected {}, got {}",
                    session_id, session.username, session.client_ip, client_ip
                );
                return None;
            }

            // Check user agent if enforced
            if self.config.enforce_user_agent && session.user_agent != user_agent {
                warn!(
                    "Session {} user agent mismatch for user '{}'",
                    session_id, session.username
                );
                return None;
            }

            // Update last active
            session.touch();
            Some(session.clone())
        } else {
            None
        }
    }

    /// Invalidate a session (logout).
    pub async fn invalidate_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(session_id) {
            info!(
                "Invalidated session {} for user '{}'",
                session_id, session.username
            );
        }
    }

    /// Invalidate all sessions for a user.
    pub async fn invalidate_all_user_sessions(&self, username: &str) {
        let mut sessions = self.sessions.write().await;
        let to_remove: Vec<String> = sessions
            .values()
            .filter(|s| s.username == username)
            .map(|s| s.session_id.clone())
            .collect();

        for id in &to_remove {
            sessions.remove(id);
        }

        if !to_remove.is_empty() {
            info!(
                "Invalidated {} sessions for user '{}'",
                to_remove.len(),
                username
            );
        }
    }

    /// Get all active sessions for a user.
    pub async fn get_user_sessions(&self, username: &str) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.username == username && s.is_valid())
            .cloned()
            .collect()
    }

    /// Clean up expired sessions.
    async fn cleanup_expired(sessions: &Arc<RwLock<HashMap<String, Session>>>) {
        let mut sessions = sessions.write().await;
        let before = sessions.len();
        sessions.retain(|_, session| session.is_valid());
        let cleaned = before - sessions.len();
        if cleaned > 0 {
            info!("Session cleanup: removed {} expired sessions", cleaned);
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionConfig::default())
    }
}
