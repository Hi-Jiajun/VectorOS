use axum::http::StatusCode;
use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tower::{Layer, Service};
use tracing::warn;

/// Rate bucket tracking requests within a time window.
#[derive(Clone, Debug)]
pub struct RateBucket {
    pub count: u32,
    pub window_start: Instant,
}

/// Rate limit configuration and shared state.
#[derive(Clone)]
pub struct RateLimitState {
    buckets: std::sync::Arc<Mutex<HashMap<String, RateBucket>>>,
    /// Maximum login attempts per window (default: 10).
    pub login_limit: u32,
    /// Maximum read (GET/HEAD) requests per window (default: 120).
    pub api_read_limit: u32,
    /// Maximum write (POST/PUT/DELETE) requests per window (default: 30).
    pub api_write_limit: u32,
    /// Window duration in seconds (default: 60).
    pub window_secs: u64,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            buckets: std::sync::Arc::new(Mutex::new(HashMap::new())),
            login_limit: 10,
            api_read_limit: 120,
            api_write_limit: 30,
            window_secs: 60,
        }
    }
}

/// Extract client IP address from request headers.
///
/// Checks `X-Forwarded-For`, `X-Real-IP` headers in order, falling back
/// to a zero address for direct connections.
pub fn extract_client_ip(headers: &axum::http::HeaderMap) -> IpAddr {
    // Try X-Forwarded-For first (most common behind proxies)
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            if let Some(first_ip) = s.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    // Try X-Real-IP
    if let Some(xri) = headers.get("x-real-ip") {
        if let Ok(s) = xri.to_str() {
            if let Ok(ip) = s.parse::<IpAddr>() {
                return ip;
            }
        }
    }
    // Fallback for direct connections
    IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))
}

// ── Tower Layer / Service implementation ──────────────────────────────

/// Layer that adds rate limiting to a service.
#[derive(Clone)]
pub struct RateLimitLayer {
    state: RateLimitState,
}

impl RateLimitLayer {
    pub fn new(state: RateLimitState) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// Tower Service that enforces per-IP rate limits.
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    state: RateLimitState,
}

impl<S> Service<axum::http::Request<axum::body::Body>> for RateLimitService<S>
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
        let client_ip = extract_client_ip(request.headers());
        let path = request.uri().path().to_string();
        let method = request.method().to_string();

        // Skip rate limiting for health check, WebSocket, and non-API routes
        if path == "/api/health" || path == "/ws" || !path.starts_with("/api/") {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(request).await });
        }

        let limit = if path == "/api/auth/login" {
            self.state.login_limit
        } else if method == "GET" || method == "HEAD" || method == "OPTIONS" {
            self.state.api_read_limit
        } else {
            self.state.api_write_limit
        };
        let window = Duration::from_secs(self.state.window_secs);

        // Check rate limit
        let rejected = {
            let mut buckets = match self.state.buckets.lock() {
                Ok(b) => b,
                Err(_) => {
                    // Lock poisoned, allow through
                    let mut inner = self.inner.clone();
                    return Box::pin(async move { inner.call(request).await });
                }
            };

            let now = Instant::now();
            let tier = if path == "/api/auth/login" {
                "login"
            } else if method == "GET" || method == "HEAD" || method == "OPTIONS" {
                "read"
            } else {
                "write"
            };
            let key = format!("{}:{}", client_ip, tier);

            let entry = buckets.entry(key).or_insert(RateBucket {
                count: 0,
                window_start: now,
            });

            // Reset window if expired
            if now.duration_since(entry.window_start) > window {
                entry.count = 0;
                entry.window_start = now;
            }

            entry.count += 1;

            if entry.count > limit {
                let elapsed = now.duration_since(entry.window_start).as_secs();
                let retry_after = if elapsed < self.state.window_secs {
                    self.state.window_secs - elapsed
                } else {
                    self.state.window_secs
                };

                warn!(
                    "Rate limit exceeded for {} on {} {} ({} requests in window)",
                    client_ip, method, path, entry.count
                );

                Some(retry_after)
            } else {
                None
            }
        };

        if let Some(retry_after) = rejected {
            let mut response = axum::response::Response::new(axum::body::Body::from(format!(
                "{{\"error\":\"Rate limit exceeded\",\"retry_after\":{}}}",
                retry_after
            )));
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            if let Ok(val) = retry_after.to_string().parse() {
                response.headers_mut().insert("retry-after", val);
            }
            if let Ok(val) = "application/json".parse() {
                response.headers_mut().insert("content-type", val);
            }
            return Box::pin(async move { Ok(response) });
        }

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(request).await })
    }
}

/// Spawn a background task that periodically cleans up stale rate limit entries.
pub fn start_cleanup_task(state: RateLimitState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;
            if let Ok(mut buckets) = state.buckets.lock() {
                let now = Instant::now();
                let window = Duration::from_secs(state.window_secs * 2);
                let before = buckets.len();
                buckets.retain(|_, entry| now.duration_since(entry.window_start) < window);
                let cleaned = before - buckets.len();
                if cleaned > 0 {
                    tracing::info!("Rate limiter cleanup: removed {} stale entries", cleaned);
                }
            }
        }
    });
}
