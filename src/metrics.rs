use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::debug;

/// Metrics collector for kube-depod
#[derive(Clone)]
pub struct Metrics {
    /// Total pods evaluated
    pub total_pods_evaluated: Arc<AtomicU64>,
    /// Total pods deleted
    pub total_pods_deleted: Arc<AtomicU64>,
    /// Total policy matches
    pub total_policy_matches: Arc<AtomicU64>,
    /// Total evaluation errors
    pub total_evaluation_errors: Arc<AtomicU64>,
    /// Total rate limit hits
    pub total_rate_limited: Arc<AtomicU64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_pods_evaluated: Arc::new(AtomicU64::new(0)),
            total_pods_deleted: Arc::new(AtomicU64::new(0)),
            total_policy_matches: Arc::new(AtomicU64::new(0)),
            total_evaluation_errors: Arc::new(AtomicU64::new(0)),
            total_rate_limited: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn increment_pods_evaluated(&self) {
        self.total_pods_evaluated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_pods_deleted(&self) {
        self.total_pods_deleted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_policy_matches(&self) {
        self.total_policy_matches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_evaluation_errors(&self) {
        self.total_evaluation_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_rate_limited(&self) {
        self.total_rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_pods_evaluated(&self) -> u64 {
        self.total_pods_evaluated.load(Ordering::Relaxed)
    }

    pub fn get_pods_deleted(&self) -> u64 {
        self.total_pods_deleted.load(Ordering::Relaxed)
    }

    pub fn get_policy_matches(&self) -> u64 {
        self.total_policy_matches.load(Ordering::Relaxed)
    }

    pub fn get_evaluation_errors(&self) -> u64 {
        self.total_evaluation_errors.load(Ordering::Relaxed)
    }

    pub fn get_rate_limited(&self) -> u64 {
        self.total_rate_limited.load(Ordering::Relaxed)
    }

    /// Generate metrics output in Prometheus format
    pub fn prometheus_format(&self) -> String {
        format!(
            "# HELP kube_depod_pods_evaluated_total Total number of pods evaluated
# TYPE kube_depod_pods_evaluated_total counter
kube_depod_pods_evaluated_total {{}} {}

# HELP kube_depod_pods_deleted_total Total number of pods deleted
# TYPE kube_depod_pods_deleted_total counter
kube_depod_pods_deleted_total {{}} {}

# HELP kube_depod_policy_matches_total Total number of policy matches
# TYPE kube_depod_policy_matches_total counter
kube_depod_policy_matches_total {{}} {}

# HELP kube_depod_evaluation_errors_total Total number of evaluation errors
# TYPE kube_depod_evaluation_errors_total counter
kube_depod_evaluation_errors_total {{}} {}

# HELP kube_depod_rate_limited_total Total number of rate limit hits
# TYPE kube_depod_rate_limited_total counter
kube_depod_rate_limited_total {{}} {}
",
            self.get_pods_evaluated(),
            self.get_pods_deleted(),
            self.get_policy_matches(),
            self.get_evaluation_errors(),
            self.get_rate_limited()
        )
    }

    pub fn log_status(&self) {
        debug!(
            "Metrics - Evaluated: {}, Deleted: {}, Matches: {}, Errors: {}, Rate Limited: {}",
            self.get_pods_evaluated(),
            self.get_pods_deleted(),
            self.get_policy_matches(),
            self.get_evaluation_errors(),
            self.get_rate_limited()
        );
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        assert_eq!(metrics.get_pods_evaluated(), 0);
        assert_eq!(metrics.get_pods_deleted(), 0);
    }

    #[test]
    fn test_metrics_increment() {
        let metrics = Metrics::new();
        metrics.increment_pods_evaluated();
        metrics.increment_pods_evaluated();
        metrics.increment_pods_deleted();

        assert_eq!(metrics.get_pods_evaluated(), 2);
        assert_eq!(metrics.get_pods_deleted(), 1);
    }

    #[test]
    fn test_prometheus_format() {
        let metrics = Metrics::new();
        metrics.increment_pods_evaluated();
        metrics.increment_pods_deleted();
        metrics.increment_policy_matches();

        let output = metrics.prometheus_format();
        assert!(output.contains("kube_depod_pods_evaluated_total"));
        assert!(output.contains("1"));
    }
}
