use chrono::Utc;
use crate::crd::{DepodPolicyStatus, PolicyCondition, When, validate_when};

/// Status updater for DepodPolicy
///
/// Provides high-level APIs to safely update DepodPolicyStatus fields,
/// preventing direct manipulation and ensuring consistency across the codebase.
pub struct StatusUpdater;

impl StatusUpdater {
    /// Record a pod evaluation
    pub fn record_pod_evaluated(status: &mut DepodPolicyStatus) {
        status.pods_evaluated += 1;
        Self::update_last_observed_time(status);
    }

    /// Record a pod that matched the policy
    pub fn record_policy_match(status: &mut DepodPolicyStatus) {
        status.pods_matched += 1;
        status.pods_evaluated += 1;
        Self::update_last_observed_time(status);
    }

    /// Record a pod deletion/eviction
    pub fn record_pod_deleted(status: &mut DepodPolicyStatus) {
        status.pods_deleted += 1;
        Self::update_last_observed_time(status);
    }

    /// Record an evaluation error
    pub fn record_evaluation_error(status: &mut DepodPolicyStatus) {
        status.evaluation_errors += 1;
        Self::update_last_observed_time(status);
    }

    /// Update policy condition (replaces existing condition of same type)
    pub fn update_condition(status: &mut DepodPolicyStatus, condition: PolicyCondition) {
        // Remove existing condition of the same type
        status.conditions.retain(|c| c.condition_type != condition.condition_type);
        // Add new condition
        status.conditions.push(condition);
    }

    /// Update last observed time to now
    pub fn update_last_observed_time(status: &mut DepodPolicyStatus) {
        status.last_observed_time = Some(
            Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        );
    }

    /// Reset all counters (useful for policy recreation)
    pub fn reset_counters(status: &mut DepodPolicyStatus) {
        status.pods_evaluated = 0;
        status.pods_matched = 0;
        status.pods_deleted = 0;
        status.evaluation_errors = 0;
    }

    /// Initialize status with Ready condition
    pub fn initialize(status: &mut DepodPolicyStatus) {
        status.conditions.clear();
        status.conditions.push(PolicyCondition::ready());
        Self::update_last_observed_time(status);
    }

    /// Mark policy as invalid CEL
    pub fn mark_invalid_cel(status: &mut DepodPolicyStatus, message: impl Into<String>) {
        Self::update_condition(status, PolicyCondition::invalid_cel(message));
        Self::update_last_observed_time(status);
    }

    /// Mark policy as invalid spec
    pub fn mark_invalid_spec(status: &mut DepodPolicyStatus, message: impl Into<String>) {
        Self::update_condition(status, PolicyCondition::invalid_spec(message));
        Self::update_last_observed_time(status);
    }

    /// Mark policy as ready
    pub fn mark_ready(status: &mut DepodPolicyStatus) {
        Self::update_condition(status, PolicyCondition::ready());
        Self::update_last_observed_time(status);
    }

    /// Validate When condition and update status with result
    ///
    /// If validation fails, updates status with InvalidSpec condition.
    /// Returns the validation result.
    pub fn validate_and_update_when(
        status: &mut DepodPolicyStatus,
        when: &When,
    ) -> crate::Result<()> {
        match validate_when(when) {
            Ok(()) => {
                // Only mark ready if no other conditions are present
                if status.conditions.is_empty() {
                    Self::mark_ready(status);
                }
                Ok(())
            }
            Err(e) => {
                Self::mark_invalid_spec(status, e.to_string());
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_policy_match() {
        let mut status = DepodPolicyStatus::default();
        StatusUpdater::record_policy_match(&mut status);

        assert_eq!(status.pods_evaluated, 1);
        assert_eq!(status.pods_matched, 1);
        assert!(status.last_observed_time.is_some());
    }

    #[test]
    fn test_record_pod_deleted() {
        let mut status = DepodPolicyStatus::default();
        StatusUpdater::record_pod_deleted(&mut status);

        assert_eq!(status.pods_deleted, 1);
        assert!(status.last_observed_time.is_some());
    }

    #[test]
    fn test_update_condition_replaces_existing() {
        let mut status = DepodPolicyStatus::default();
        
        let cond1 = PolicyCondition::invalid_cel("error 1");
        StatusUpdater::update_condition(&mut status, cond1);
        assert_eq!(status.conditions.len(), 1);

        let cond2 = PolicyCondition::invalid_cel("error 2");
        StatusUpdater::update_condition(&mut status, cond2);
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].message, Some("error 2".to_string()));
    }

    #[test]
    fn test_initialize() {
        let mut status = DepodPolicyStatus::default();
        status.pods_evaluated = 5;
        
        StatusUpdater::initialize(&mut status);
        
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].condition_type, "Ready");
        assert!(status.last_observed_time.is_some());
    }

    #[test]
    fn test_reset_counters() {
        let mut status = DepodPolicyStatus {
            pods_evaluated: 10,
            pods_matched: 5,
            pods_deleted: 3,
            evaluation_errors: 2,
            ..Default::default()
        };

        StatusUpdater::reset_counters(&mut status);

        assert_eq!(status.pods_evaluated, 0);
        assert_eq!(status.pods_matched, 0);
        assert_eq!(status.pods_deleted, 0);
        assert_eq!(status.evaluation_errors, 0);
    }

    #[test]
    fn test_validate_and_update_when_valid_cel() {
        use crate::crd::{ConditionType, When};

        let mut status = DepodPolicyStatus::default();
        let when = When {
            condition_type: ConditionType::CEL,
            expression: Some("status.phase == 'Succeeded'".to_string()),
            ttl_seconds: None,
        };

        let result = StatusUpdater::validate_and_update_when(&mut status, &when);
        
        assert!(result.is_ok());
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].condition_type, "Ready");
    }

    #[test]
    fn test_validate_and_update_when_invalid_cel() {
        use crate::crd::{ConditionType, When};

        let mut status = DepodPolicyStatus::default();
        let when = When {
            condition_type: ConditionType::CEL,
            expression: None, // Missing required expression
            ttl_seconds: None,
        };

        let result = StatusUpdater::validate_and_update_when(&mut status, &when);
        
        assert!(result.is_err());
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].condition_type, "InvalidSpec");
        assert!(status.conditions[0].message.as_ref().unwrap().contains("expression required"));
    }

    #[test]
    fn test_validate_and_update_when_valid_builtin() {
        use crate::crd::{ConditionType, When};

        let mut status = DepodPolicyStatus::default();
        let when = When {
            condition_type: ConditionType::Builtin,
            expression: None,
            ttl_seconds: Some(600),
        };

        let result = StatusUpdater::validate_and_update_when(&mut status, &when);
        
        assert!(result.is_ok());
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].condition_type, "Ready");
    }

    #[test]
    fn test_validate_and_update_when_invalid_builtin() {
        use crate::crd::{ConditionType, When};

        let mut status = DepodPolicyStatus::default();
        let when = When {
            condition_type: ConditionType::Builtin,
            expression: None,
            ttl_seconds: Some(-1), // Invalid: must be positive
        };

        let result = StatusUpdater::validate_and_update_when(&mut status, &when);
        
        assert!(result.is_err());
        assert_eq!(status.conditions.len(), 1);
        assert_eq!(status.conditions[0].condition_type, "InvalidSpec");
        assert!(status.conditions[0].message.as_ref().unwrap().contains("must be a positive integer"));
    }
}
