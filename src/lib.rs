pub mod controller;
pub mod crd;
pub mod engine;
pub mod error;
pub mod metrics;
pub mod rate_limiter;
pub mod server;

pub use error::{Error, Result};

// Shared context for the operator
use arc_swap::ArcSwap;
use crd::DepodPolicy;
use engine::CelEvaluator;
use kube::Client;
use metrics::Metrics;
use rate_limiter::RateLimiter;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct Context {
    pub client: Client,
    pub metrics: Arc<Metrics>,
    pub evaluator: Arc<CelEvaluator>,
    /// Lock-free policy cache using ArcSwap
    /// Enables concurrent reads without blocking writes (no RwLock contention)
    pub policies: Arc<ArcSwap<Vec<DepodPolicy>>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub operator_pod_name: Arc<String>,
    /// Periodic resync interval for cron check feature
    /// Some(Duration) = enabled with specified interval
    /// None = disabled
    pub periodic_resync_interval: Option<Duration>,
    /// Concurrency limit for pod patch operations
    /// Limits parallel API requests to protect API server when touching pods
    pub pod_patch_concurrency_limit: usize,
}

#[cfg(test)]
mod tests {
    use crate::crd::{ActionType, ConditionType, DepodPolicySpec, Limits, Match, Then, Trigger, When};
    use std::collections::BTreeSet;

    #[test]
    fn test_policy_validation_builtin_valid() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["ttl-10m".to_string()]),
            },
            when: When {
                condition_type: ConditionType::Builtin,
                expression: None,
                ttl_seconds: Some(600),
            },
            then: Then {
                action_type: ActionType::Delete,
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
    }

    #[test]
    fn test_policy_validation_cel_valid() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["auto-cleanup".to_string()]),
            },
            when: When {
                condition_type: ConditionType::CEL,
                expression: Some("status.phase == 'Succeeded'".to_string()),
                ttl_seconds: None,
            },
            then: Then {
                action_type: ActionType::Delete,
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
    }

    #[test]
    fn test_policy_validation_cel_missing_expression() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["auto-cleanup".to_string()]),
            },
            when: When {
                condition_type: ConditionType::CEL,
                expression: None,
                ttl_seconds: None,
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: None,
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        let err = spec.validate().unwrap_err();
        assert!(err.contains("when.expression required"));
    }

    #[test]
    fn test_policy_validation_cel_with_ttl_seconds_fails() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["auto-cleanup".to_string()]),
            },
            when: When {
                condition_type: ConditionType::CEL,
                expression: Some("status.phase == 'Succeeded'".to_string()),
                ttl_seconds: Some(600), // Should not be set for CEL
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: None,
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        let err = spec.validate().unwrap_err();
        assert!(err.contains("when.ttlSeconds must not be set for CEL type"));
    }

    #[test]
    fn test_policy_validation_builtin_missing_ttl_seconds() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["ttl-10m".to_string()]),
            },
            when: When {
                condition_type: ConditionType::Builtin,
                expression: None,
                ttl_seconds: None, // Missing required TTL
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: None,
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        let err = spec.validate().unwrap_err();
        assert!(err.contains("when.ttlSeconds required"));
    }

    #[test]
    fn test_policy_validation_builtin_with_expression_fails() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["ttl-10m".to_string()]),
            },
            when: When {
                condition_type: ConditionType::Builtin,
                expression: Some("status.phase == 'Failed'".to_string()), // Should not be set for Builtin
                ttl_seconds: Some(600),
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: None,
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        let err = spec.validate().unwrap_err();
        assert!(err.contains("when.expression must not be set for Builtin type"));
    }

    #[test]
    fn test_policy_validation_invalid_ttl_value() {
        let spec = DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "kube-depod/policy".to_string(),
                annotation_values: BTreeSet::from(["ttl-10m".to_string()]),
            },
            when: When {
                condition_type: ConditionType::Builtin,
                expression: None,
                ttl_seconds: Some(0), // TTL must be positive
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: None,
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        };

        let err = spec.validate().unwrap_err();
        assert!(err.contains("when.ttlSeconds must be a positive integer"));
    }
}
