//! In-memory cache with TTL for reducing repeated expensive computations.
//!
//! Used to cache VPP system info, performance metrics, and interface data
//! that are queried frequently by both REST API and WebSocket broadcaster.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A cached value with an expiration time.
struct CachedEntry<T> {
    value: T,
    expires_at: Instant,
}

/// Thread-safe cache with per-key TTL.
pub struct Cache {
    entries: RwLock<HashMap<String, CachedEntry<Vec<u8>>>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Get a cached value if it exists and has not expired.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let entries = self.entries.read().ok()?;
        if let Some(entry) = entries.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Insert a value with a TTL.
    pub fn insert(&self, key: &str, value: Vec<u8>, ttl: Duration) {
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(
                key.to_string(),
                CachedEntry {
                    value,
                    expires_at: Instant::now() + ttl,
                },
            );
        }
    }

    /// Remove expired entries (garbage collection).
    pub fn gc(&self) {
        if let Ok(mut entries) = self.entries.write() {
            let now = Instant::now();
            entries.retain(|_, entry| now < entry.expires_at);
        }
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

/// Global cache instance.
static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();

pub fn global() -> &'static Cache {
    CACHE.get_or_init(Cache::new)
}
