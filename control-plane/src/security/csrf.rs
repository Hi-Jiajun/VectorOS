use axum::http::StatusCode;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::RwLock;
use tower::{Layer, Service};
use tracing::warn;

/// CSRF protection state that manages tokens per user session.
///
/// The system uses the double-submit pattern: the server generates a CSRF
/// token during login, and the client must include it in the `X-CSRF-Token`
/// header on state-changing requests.
#[derive(Clone)]
pub struct CsrfState {
    /// Maps username to their current CSRF token.
    tokens: Arc<RwLock<HashMap<String, String>>>,
}

impl CsrfState {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate and store a new CSRF token for a user.
    pub async fn generate_token(&self, username: &str) -> String {
        let token = generate_random_token();
        let mut tokens = self.tokens.write().await;
        tokens.insert(username.to_string(), token.clone());
        token
    }

    /// Validate a CSRF token for a user.
    pub async fn validate_token(&self, username: &str, token: &str) -> bool {
        let tokens = self.tokens.read().await;
        match tokens.get(username) {
            Some(expected) => {
                // Constant-time comparison to prevent timing attacks
                constant_time_eq(token.as_bytes(), expected.as_bytes())
            }
            None => false,
        }
    }

    /// Remove CSRF token for a user (e.g., on logout).
    pub async fn remove_token(&self, username: &str) {
        let mut tokens = self.tokens.write().await;
        tokens.remove(username);
    }
}

impl Default for CsrfState {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a cryptographically random hex token (32 bytes = 64 hex chars).
pub fn generate_random_token() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        file.read_exact(&mut bytes)
            .expect("Failed to read from /dev/urandom");
    } else {
        warn!("Could not open /dev/urandom, using fallback random generation");
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::Instant::now().hash(&mut hasher);
        let hash = hasher.finish();
        bytes[..8].copy_from_slice(&hash.to_le_bytes());
        for i in (8..32).step_by(8) {
            let end = std::cmp::min(i + 8, 32);
            bytes[i..end].copy_from_slice(&hash.to_le_bytes()[..end - i]);
        }
    }
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// ── Tower Layer / Service implementation ──────────────────────────────

/// Layer that adds CSRF protection to a service.
#[derive(Clone)]
pub struct CsrfLayer {
    state: CsrfState,
}

impl CsrfLayer {
    pub fn new(state: CsrfState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for CsrfLayer {
    type Service = CsrfService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CsrfService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Tower Service that validates CSRF tokens on state-changing requests.
#[derive(Clone)]
pub struct CsrfService<S> {
    inner: S,
    state: CsrfState,
}

impl<S> Service<axum::http::Request<axum::body::Body>> for CsrfService<S>
where
    S: Service<axum::http::Request<axum::body::Body>, Response = axum::response::Response>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::http::Request<axum::body::Body>) -> Self::Future {
        let path = request.uri().path().to_string();
        let method = request.method().clone();

        // Skip CSRF for non-state-changing methods
        if method == "GET" || method == "HEAD" || method == "OPTIONS" {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(request).await });
        }

        // Skip CSRF for login endpoint (no session yet) and health check
        if path == "/api/auth/login" || path == "/api/health" || !path.starts_with("/api/") {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(request).await });
        }

        // Extract CSRF token from header
        let csrf_token = request
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Extract user from JWT claims (set by auth middleware)
        let user = request
            .extensions()
            .get::<crate::auth::Claims>()
            .map(|c| c.sub.clone())
            .unwrap_or_default();

        let state = self.state.clone();

        if csrf_token.is_empty() || user.is_empty() {
            let status = if user.is_empty() {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            };
            let mut response = axum::response::Response::new(axum::body::Body::from(
                format!("{{\"error\":\"{}\"}}", if status == StatusCode::FORBIDDEN {
                    "CSRF token missing. Include X-CSRF-Token header."
                } else {
                    "Authentication required"
                }),
            ));
            *response.status_mut() = status;
            if let Ok(val) = "application/json".parse() {
                response.headers_mut().insert("content-type", val);
            }
            return Box::pin(async move { Ok(response) });
        }

        let user_clone = user.clone();
        let token_clone = csrf_token.clone();
        let method_clone = method.clone();
        let path_clone = path.clone();
        let mut inner_clone = self.inner.clone();

        Box::pin(async move {
            if !state.validate_token(&user_clone, &token_clone).await {
                warn!(
                    "CSRF token validation failed for user '{}' on {} {}",
                    user_clone, method_clone, path_clone
                );
                let mut response = axum::response::Response::new(axum::body::Body::from(
                    "{\"error\":\"Invalid CSRF token\"}",
                ));
                *response.status_mut() = StatusCode::FORBIDDEN;
                if let Ok(val) = "application/json".parse() {
                    response.headers_mut().insert("content-type", val);
                }
                return Ok(response);
            }

            inner_clone.call(request).await
        })
    }
}
