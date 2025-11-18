use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Internal token bucket implementation
#[derive(Debug)]
struct TokenBucket {
    max_per_minute: u64,
    last_reset: AtomicU64,
    tokens: AtomicU64,
}

impl TokenBucket {
    fn new(max_per_minute: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            max_per_minute,
            last_reset: AtomicU64::new(now),
            tokens: AtomicU64::new(max_per_minute),
        }
    }

    fn allow(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let last_reset = self.last_reset.load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(last_reset);

        // Reset tokens every minute
        if elapsed >= 60 {
            self.last_reset.store(now, Ordering::Relaxed);
            self.tokens.store(self.max_per_minute, Ordering::Relaxed);
        }

        // Try to consume a token
        let mut tokens = self.tokens.load(Ordering::Relaxed);
        while tokens > 0 {
            match self.tokens.compare_exchange_weak(
                tokens,
                tokens - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    debug!("TokenBucket: {} tokens remaining", tokens - 1);
                    return true;
                }
                Err(actual) => tokens = actual,
            }
        }

        debug!("TokenBucket: limit exceeded");
        false
    }

    fn get_tokens(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Rate limiter managing global and per-policy limits
#[derive(Clone)]
pub struct RateLimiter {
    global_limiter: Arc<TokenBucket>,
    policy_limiters: Arc<DashMap<String, Arc<TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter with global max operations per minute
    pub fn new(global_max_per_minute: u64) -> Self {
        Self {
            global_limiter: Arc::new(TokenBucket::new(global_max_per_minute)),
            policy_limiters: Arc::new(DashMap::new()),
        }
    }

    /// Check if operation is allowed by global limit
    pub fn allow_global(&self) -> bool {
        self.global_limiter.allow()
    }

    /// Check if operation is allowed by policy-specific limit
    /// Returns true if allowed (or if limit is None), false otherwise
    pub fn allow_policy(&self, policy_name: &str, max_per_minute: Option<i32>) -> bool {
        let limit = match max_per_minute {
            Some(l) if l > 0 => l as u64,
            _ => return true, // No limit or invalid limit implies allowed
        };

        // Get or create policy limiter
        // We use a fast path for existing limiters
        if let Some(limiter) = self.policy_limiters.get(policy_name) {
            // If the limit config changed, we might need to update it, but for now
            // we assume the limit stays relatively stable or we accept the old limit until restart/re-creation.
            // To strictly support dynamic updates, we'd need to check max_per_minute.
            // For simplicity and performance, if the limit is different, we replace it.
            if limiter.max_per_minute != limit {
                // Limit changed, replace bucket
                drop(limiter); // Release read lock
                let new_bucket = Arc::new(TokenBucket::new(limit));
                self.policy_limiters.insert(policy_name.to_string(), new_bucket.clone());
                return new_bucket.allow();
            }
            return limiter.allow();
        }

        // Create new limiter
        let new_bucket = Arc::new(TokenBucket::new(limit));
        self.policy_limiters.insert(policy_name.to_string(), new_bucket.clone());
        new_bucket.allow()
    }

    /// Legacy allow method for backward compatibility (checks global only)
    pub fn allow(&self) -> bool {
        self.allow_global()
    }

    /// Get current global token count (for testing/metrics)
    pub fn get_tokens(&self) -> u64 {
        self.global_limiter.get_tokens()
    }

    /// Get global maximum operations per minute (for testing/metrics)
    pub fn get_max_per_minute(&self) -> u64 {
        self.global_limiter.max_per_minute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_rate_limit() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.allow_global());
        assert!(limiter.allow_global());
        assert!(limiter.allow_global());
        assert!(!limiter.allow_global());
    }

    #[test]
    fn test_policy_rate_limit() {
        let limiter = RateLimiter::new(100); // High global limit
        let policy_name = "test-policy";
        let policy_limit = Some(2);

        assert!(limiter.allow_policy(policy_name, policy_limit));
        assert!(limiter.allow_policy(policy_name, policy_limit));
        assert!(!limiter.allow_policy(policy_name, policy_limit));

        // Another policy should be independent
        let policy2 = "other-policy";
        assert!(limiter.allow_policy(policy2, policy_limit));
    }

    #[test]
    fn test_policy_limit_update() {
        let limiter = RateLimiter::new(100);
        let policy_name = "dynamic-policy";
        
        // Start with limit 1
        assert!(limiter.allow_policy(policy_name, Some(1)));
        assert!(!limiter.allow_policy(policy_name, Some(1)));

        // Update to limit 10
        assert!(limiter.allow_policy(policy_name, Some(10)));
    }
}
