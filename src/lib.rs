pub mod controller;
pub mod crd;
pub mod engine;
pub mod error;
pub mod metrics;
pub mod rate_limiter;
pub mod server;

pub use error::{Error, Result};

// Shared context for the operator
use crd::DepodPolicy;
use engine::CelEvaluator;
use kube::Client;
use metrics::Metrics;
use rate_limiter::RateLimiter;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Context {
    pub client: Client,
    pub metrics: Arc<Metrics>,
    pub evaluator: Arc<CelEvaluator>,
    pub policies: Arc<RwLock<Vec<DepodPolicy>>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub operator_pod_name: Arc<String>,
    /// Periodic resync interval for cron check feature
    /// Some(Duration) = enabled with specified interval
    /// None = disabled
    pub periodic_resync_interval: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use crate::crd::{DepodPolicySpec, Limits, Match, Then, Trigger, When};

    #[test]
    fn test_policy_validation() {
        let mut spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: vec!["ttl-10m".to_string()],
            },
            when: When {
                condition_type: "Builtin".to_string(),
                expression: None,
                ttl_seconds: Some(600),
            },
            then: Then {
                action_type: "Delete".to_string(),
                grace_period_seconds: Some(30),
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: Some(20),
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        assert!(spec.validate().is_ok());

        // Test invalid condition type for CEL without expression
        spec.when.condition_type = "CEL".to_string();
        spec.when.expression = None;
        assert!(spec.validate().is_err());

        // Test valid CEL
        spec.when.expression = Some("metadata.age > 600".to_string());
        assert!(spec.validate().is_ok());

        // Test invalid action type
        spec.then.action_type = "Invalid".to_string();
        assert!(spec.validate().is_err());
    }
}
