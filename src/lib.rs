pub mod controller;
pub mod crd;
pub mod engine;
pub mod error;
pub mod metrics;
pub mod rate_limiter;
pub mod server;

pub use error::{Error, Result};

#[cfg(test)]
mod tests {
    use crate::crd::{DepodPolicySpec, Trigger, Match, When, Then, Limits};

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
                grace_seconds: Some(30),
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: Some(20),
                protect_system_namespaces: true,
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
