use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use chrono::Utc;
use crate::Error;

/// Condition type for policy evaluation
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionType {
    /// CEL (Common Expression Language) condition
    CEL,
    /// Built-in TTL (Time To Live) condition
    Builtin,
}

/// Action type to perform when condition is met
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ActionType {
    /// Delete the pod directly
    Delete,
    /// Evict the pod (respects Pod Disruption Budgets)
    Evict,
}

/// DepodPolicy CRD for automated Pod cleanup
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "kube-depod.io",
    version = "v1alpha1",
    kind = "DepodPolicy",
    namespaced,
    status = "DepodPolicyStatus"
)]
pub struct DepodPolicySpec {
    /// Match namespace and pod selectors
    #[serde(rename = "match")]
    pub match_: Match,

    /// Trigger condition (annotation-based)
    pub trigger: Trigger,

    /// When condition (expression for evaluation)
    pub when: When,

    /// Then action to perform
    pub then: Then,

    /// Safety limits
    pub limits: Limits,
}

/// Match specifies which pods are affected by this policy
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Match {
    /// Namespace selector (matchNames or matchExpressions)
    #[serde(default, rename = "namespaceSelector")]
    pub namespace_selector: Option<NamespaceSelector>,

    /// Pod label selector
    #[serde(default, rename = "podSelector")]
    pub pod_selector: Option<PodSelector>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct NamespaceSelector {
    /// Match specific namespace names
    #[serde(default, rename = "matchNames")]
    pub match_names: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct PodSelector {
    /// Match pods by labels
    #[serde(default, rename = "matchLabels")]
    pub match_labels: BTreeMap<String, String>,
}

/// Trigger specifies when the policy should be checked
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Trigger {
    /// Annotation key to look for
    #[serde(rename = "annotationKey")]
    pub annotation_key: String,

    /// Accepted annotation values (BTreeSet for O(log n) lookup)
    #[serde(rename = "annotationValues")]
    pub annotation_values: BTreeSet<String>,
}

/// When specifies the evaluation rule
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct When {
    /// Type of condition: CEL or Builtin
    #[serde(rename = "type")]
    pub condition_type: ConditionType,

    /// CEL expression or condition parameters
    #[serde(default)]
    pub expression: Option<String>,

    /// TTL seconds for builtin TTL type
    #[serde(default, rename = "ttlSeconds")]
    pub ttl_seconds: Option<i64>,
}

/// Then specifies what to do when condition is met
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Then {
    /// Type of action: Delete, Evict, etc.
    #[serde(rename = "type")]
    pub action_type: ActionType,

    /// Grace period for deletion/eviction in seconds
    #[serde(default, rename = "gracePeriodSeconds")]
    pub grace_period_seconds: Option<i64>,

    /// Dry run mode (don't actually delete)
    #[serde(default, rename = "dryRun")]
    pub dry_run: bool,
}

/// Limits defines safety guardrails for pod deletion
///
/// Note: `maxDeletesPerMinute` is typically not set here. Instead, the global
/// rate limit is configured via the `RATE_LIMIT_PER_MINUTE` environment variable
/// (default: 20). If you need per-policy rate limits in the future, set this field;
/// otherwise, leave it unset to use the global rate limit.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Limits {
    /// Max pods to delete per minute for this policy
    ///
    /// Optional. If not set, the global rate limit (from RATE_LIMIT_PER_MINUTE env)
    /// is used. Reserved for future per-policy rate limiting.
    #[serde(default, rename = "maxDeletesPerMinute")]
    pub max_deletes_per_minute: Option<i32>,

    /// Protect system namespaces from deletion
    ///
    /// Default: true. When enabled, pods in system namespaces
    /// (kube-*, default) are protected.
    #[serde(default, rename = "protectSystemNamespaces")]
    pub protect_system_namespaces: bool,

    /// List of additional namespaces to exclude from deletion
    ///
    /// These namespaces will be protected from pod deletion by this policy,
    /// regardless of pod labels or other conditions.
    #[serde(default, rename = "excludedNamespaces")]
    pub excluded_namespaces: Option<Vec<String>>,
}

/// Status of a DepodPolicy
#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
pub struct DepodPolicyStatus {
    /// Conditions represent the latest available observations of the DepodPolicy's state
    #[serde(default)]
    pub conditions: Vec<PolicyCondition>,

    /// Last time the policy was observed and evaluated (RFC3339 format)
    #[serde(default, rename = "lastObservedTime")]
    pub last_observed_time: Option<String>,

    /// Total number of pods evaluated under this policy
    /// Uses u64 to prevent overflow in long-running operators
    #[serde(default)]
    pub pods_evaluated: u64,

    /// Number of pods that matched this policy
    /// Uses u64 to prevent overflow in long-running operators
    #[serde(default)]
    pub pods_matched: u64,

    /// Number of pods deleted/evicted by this policy
    /// Uses u64 to prevent overflow in long-running operators
    #[serde(default)]
    pub pods_deleted: u64,

    /// Any evaluation errors that occurred
    /// Uses u64 to prevent overflow in long-running operators
    #[serde(default)]
    pub evaluation_errors: u64,
}

/// PolicyCondition describes the state of a policy
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct PolicyCondition {
    /// Type of condition (e.g., "InvalidCEL", "SpecValidated", "Ready")
    #[serde(rename = "type")]
    pub condition_type: String,

    /// Status of the condition (True, False, Unknown)
    pub status: String,

    /// Last time the condition was probed (RFC3339 format)
    #[serde(rename = "lastTransitionTime", default)]
    pub last_transition_time: Option<String>,

    /// Unique, one-word, CamelCase reason for the condition's last transition
    #[serde(default)]
    pub reason: Option<String>,

    /// Human-readable message indicating details about the transition
    #[serde(default)]
    pub message: Option<String>,
}

impl PolicyCondition {
    /// Create a new InvalidCEL condition
    pub fn invalid_cel(message: impl Into<String>) -> Self {
        Self {
            condition_type: "InvalidCEL".to_string(),
            status: "True".to_string(),
            last_transition_time: Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            reason: Some("CELCompilationError".to_string()),
            message: Some(message.into()),
        }
    }

    /// Create a new InvalidSpec condition
    pub fn invalid_spec(message: impl Into<String>) -> Self {
        Self {
            condition_type: "InvalidSpec".to_string(),
            status: "True".to_string(),
            last_transition_time: Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            reason: Some("SpecValidationError".to_string()),
            message: Some(message.into()),
        }
    }

    /// Create a Ready condition
    pub fn ready() -> Self {
        Self {
            condition_type: "Ready".to_string(),
            status: "True".to_string(),
            last_transition_time: Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            reason: Some("PolicyValidated".to_string()),
            message: Some("Policy spec is valid and ready for evaluation".to_string()),
        }
    }
}

/// Validate the When condition field
///
/// Enforces type-safe constraints:
/// - CEL type: expression must be present and non-empty, ttlSeconds must be absent
/// - Builtin type: ttlSeconds must be present and positive, expression must be absent
///
/// Returns error message for status update if validation fails
pub fn validate_when(when: &When) -> crate::Result<()> {
    match when.condition_type {
        ConditionType::CEL => {
            // CEL type: expression is required
            let is_invalid = match when.expression.as_ref() {
                None => true,
                Some(s) => s.trim().is_empty(),
            };
            if is_invalid {
                return Err(Error::ValidationError("when.expression required and cannot be empty for CEL type".to_string()));
            }
            // CEL type: ttlSeconds must not be set
            if when.ttl_seconds.is_some() {
                return Err(Error::ValidationError(
                    "when.ttlSeconds must not be set for CEL type (use expression instead)".to_string()
                ));
            }
        }
        ConditionType::Builtin => {
            // Builtin type: ttlSeconds is required and must be positive
            if when.ttl_seconds.is_none() {
                return Err(Error::ValidationError("when.ttlSeconds required for Builtin type".to_string()));
            }
            if let Some(ttl) = when.ttl_seconds {
                if ttl <= 0 {
                    return Err(Error::ValidationError("when.ttlSeconds must be a positive integer".to_string()));
                }
            }
            // Builtin type: expression must not be set
            if when.expression.is_some() {
                return Err(Error::ValidationError(
                    "when.expression must not be set for Builtin type (use ttlSeconds instead)".to_string()
                ));
            }
        }
    }
    Ok(())
}

impl DepodPolicySpec {
    /// Validate the spec
    ///
    /// Enforces:
    /// - Trigger: annotation_key and annotation_values must be non-empty
    /// - When: condition type-specific validation via validate_when()
    /// - Action types are compile-time checked via enum
    /// - Grace period (if present) must be non-negative
    ///
    /// Returns `crate::Result<()>` for unified error handling
    pub fn validate(&self) -> crate::Result<()> {
        if self.trigger.annotation_key.is_empty() {
            return Err(Error::ValidationError("trigger.annotation_key cannot be empty".to_string()));
        }

        if self.trigger.annotation_values.is_empty() {
            return Err(Error::ValidationError("trigger.annotation_values cannot be empty".to_string()));
        }

        // Validate 'when' condition using dedicated function
        validate_when(&self.when)?;

        // Action type validation is now compile-time checked (no need for string matching)

        // Validate grace period if present
        if let Some(grace) = self.then.grace_period_seconds {
            if grace < 0 {
                return Err(Error::ValidationError("then.gracePeriodSeconds must be non-negative".to_string()));
            }
        }

        // Warn if max_deletes_per_minute is set (not yet implemented per-policy)
        if self.limits.max_deletes_per_minute.is_some() {
            tracing::warn!("limits.maxDeletesPerMinute is currently ignored. Global rate limit is used instead.");
        }

        Ok(())
    }
}
