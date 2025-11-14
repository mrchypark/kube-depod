use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Token bucket rate limiter for delete operations
#[derive(Clone)]
pub struct RateLimiter {
    /// Maximum number of operations per minute
    max_per_minute: u64,
    /// Last reset timestamp (in seconds)
    last_reset: Arc<AtomicU64>,
    /// Current token count
    tokens: Arc<AtomicU64>,
}

impl RateLimiter {
    /// Create a new rate limiter with max operations per minute
    pub fn new(max_per_minute: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            max_per_minute,
            last_reset: Arc::new(AtomicU64::new(now)),
            tokens: Arc::new(AtomicU64::new(max_per_minute)),
        }
    }

    /// Check if an operation is allowed, consuming a token if so
    pub fn allow(&self) -> bool {
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
                    debug!("Rate limiter: {} tokens remaining", tokens - 1);
                    return true;
                }
                Err(actual) => tokens = actual,
            }
        }

        debug!("Rate limiter: limit exceeded");
        false
    }

    /// Get current token count
    pub fn get_tokens(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }

    /// Get maximum operations per minute
    pub fn get_max_per_minute(&self) -> u64 {
        self.max_per_minute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(5);
        assert_eq!(limiter.get_max_per_minute(), 5);
        assert_eq!(limiter.get_tokens(), 5);
    }

    #[test]
    fn test_rate_limiter_allows_up_to_limit() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.allow());
        assert!(limiter.allow());
        assert!(limiter.allow());
        assert!(!limiter.allow());
    }

    #[test]
    fn test_rate_limiter_token_consumption() {
        let limiter = RateLimiter::new(2);
        assert_eq!(limiter.get_tokens(), 2);

        assert!(limiter.allow());
        assert_eq!(limiter.get_tokens(), 1);

        assert!(limiter.allow());
        assert_eq!(limiter.get_tokens(), 0);

        assert!(!limiter.allow());
        assert_eq!(limiter.get_tokens(), 0);
    }
}
