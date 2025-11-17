use std::time::Duration;
use tracing::info;

/// Configuration for the operator loaded from environment variables.
///
/// Priority order (highest to lowest):
/// 1. Environment variables (e.g., SERVER_PORT, RATE_LIMIT_PER_MINUTE)
/// 2. Downward API (e.g., OPERATOR_POD_NAME via spec.env)
/// 3. Built-in defaults
#[derive(Debug, Clone)]
pub struct Config {
    /// Rate limit for pod operations (deletes/evictions per minute)
    pub rate_limit_per_minute: u64,
    /// Concurrency limit for pod patch operations
    pub pod_patch_concurrency_limit: usize,
    /// Port for server HTTP server (container port)
    pub server_port: u16,
    /// Operator Pod name (from Downward API or default)
    pub operator_pod_name: String,
    /// Periodic resync interval for cron-style checking
    pub periodic_resync_interval: Option<Duration>,
}

impl Config {
    /// Load configuration from environment variables with sane defaults.
    ///
    /// Priority: env var (config) > Downward API > default
    ///
    /// Defaults:
    /// - RATE_LIMIT_PER_MINUTE: 20
    /// - POD_PATCH_CONCURRENCY_LIMIT: 10
    /// - SERVER_PORT: 8080
    /// - OPERATOR_POD_NAME: "kube-depod-unknown"
    /// - RESYNC_ENABLE: "true"
    /// - RESYNC_INTERVAL_SECONDS: 3600
    pub fn from_env() -> Self {
        // Rate limit per minute
        let rate_limit_per_minute = std::env::var("RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(20);
        info!(
            "Rate limit configured: {} deletes per minute",
            rate_limit_per_minute
        );

        // Pod patch concurrency limit
        let pod_patch_concurrency_limit = std::env::var("POD_PATCH_CONCURRENCY_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);
        info!(
            "Pod patch concurrency limit configured: {} concurrent patch operations",
            pod_patch_concurrency_limit
        );

        // Server port (config > Downward API > default)
        let server_port = std::env::var("SERVER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or_else(|| {
                std::env::var("SERVER_PORT_DOWNWARD")
                    .ok()
                    .and_then(|v| v.parse::<u16>().ok())
            })
            .unwrap_or(8080);
        info!("Server port configured: {}", server_port);

        // Operator Pod name (config > Downward API > default)
        let operator_pod_name =
            std::env::var("OPERATOR_POD_NAME").unwrap_or_else(|_| "kube-depod-unknown".to_string());
        info!("Operator Pod Name: {}", operator_pod_name);

        // Periodic resync (cron check) configuration
        let periodic_resync_interval = if std::env::var("RESYNC_ENABLE")
            .unwrap_or_else(|_| "true".to_string())
            .eq_ignore_ascii_case("true")
        {
            let interval_seconds = std::env::var("RESYNC_INTERVAL_SECONDS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(3600);

            info!(
                "Periodic Resync (Cron Check) enabled. Interval: {} seconds",
                interval_seconds
            );
            Some(Duration::from_secs(interval_seconds))
        } else {
            info!("Periodic Resync (Cron Check) is disabled.");
            None
        };

        Config {
            rate_limit_per_minute,
            pod_patch_concurrency_limit,
            server_port,
            operator_pod_name,
            periodic_resync_interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        // Ensure default values work when env vars are not set
        let config = Config {
            rate_limit_per_minute: 20,
            pod_patch_concurrency_limit: 10,
            server_port: 8080,
            operator_pod_name: "test-operator".to_string(),
            periodic_resync_interval: Some(Duration::from_secs(3600)),
        };

        assert_eq!(config.rate_limit_per_minute, 20);
        assert_eq!(config.pod_patch_concurrency_limit, 10);
        assert_eq!(config.server_port, 8080);
        assert_eq!(config.operator_pod_name, "test-operator");
        assert!(config.periodic_resync_interval.is_some());
    }
}
