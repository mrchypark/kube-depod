use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// DepodPolicy CRD for automated Pod cleanup
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "kube-depod.io",
    version = "v1alpha1",
    kind = "DepodPolicy",
    namespaced
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

    /// Accepted annotation values
    #[serde(rename = "annotationValues")]
    pub annotation_values: Vec<String>,
}

/// When specifies the evaluation rule
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct When {
    /// Type of condition: CEL or Builtin
    #[serde(rename = "type")]
    pub condition_type: String,

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
    pub action_type: String,

    /// Grace period for deletion/eviction in seconds
    #[serde(default, rename = "gracePeriodSeconds")]
    pub grace_period_seconds: Option<i64>,

    /// Dry run mode (don't actually delete)
    #[serde(default, rename = "dryRun")]
    pub dry_run: bool,
}

/// Limits defines safety guardrails
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Limits {
    /// Max pods to delete per minute
    #[serde(default, rename = "maxDeletesPerMinute")]
    pub max_deletes_per_minute: Option<i32>,

    /// Protect system namespaces
    #[serde(default, rename = "protectSystemNamespaces")]
    pub protect_system_namespaces: bool,

    /// List of additional namespaces to exclude from deletion
    /// Comma-separated or as a YAML list
    #[serde(default, rename = "excludedNamespaces")]
    pub excluded_namespaces: Option<Vec<String>>,
}

impl DepodPolicySpec {
    /// Validate the spec with strict type checking
    /// 
    /// Enforces:
    /// - CEL type: expression must be present, ttlSeconds must be absent
    /// - Builtin type: ttlSeconds must be present, expression must be absent
    /// - Other types: rejected
    pub fn validate(&self) -> Result<(), String> {
        if self.trigger.annotation_key.is_empty() {
            return Err("trigger.annotation_key cannot be empty".to_string());
        }

        if self.trigger.annotation_values.is_empty() {
            return Err("trigger.annotation_values cannot be empty".to_string());
        }

        // Validate 'when' condition type and fields
        match self.when.condition_type.as_str() {
            "CEL" => {
                // CEL type: expression required, ttlSeconds must be absent
                if self.when.expression.is_none() || self.when.expression.as_ref().map_or(true, |s| s.trim().is_empty()) {
                    return Err("when.expression required and cannot be empty for CEL type".to_string());
                }
                if self.when.ttl_seconds.is_some() {
                    return Err(
                        "when.ttlSeconds must not be set for CEL type (use expression instead)".to_string()
                    );
                }
            }
            "Builtin" => {
                // Builtin type: ttlSeconds required, expression must be absent
                if self.when.ttl_seconds.is_none() {
                    return Err("when.ttlSeconds required for Builtin type".to_string());
                }
                if let Some(ttl) = self.when.ttl_seconds {
                    if ttl <= 0 {
                        return Err("when.ttlSeconds must be a positive integer".to_string());
                    }
                }
                if self.when.expression.is_some() {
                    return Err(
                        "when.expression must not be set for Builtin type (use ttlSeconds instead)".to_string()
                    );
                }
            }
            _ => {
                return Err(format!(
                    "unsupported condition type '{}': must be 'CEL' or 'Builtin'",
                    self.when.condition_type
                ));
            }
        }

        // Validate 'then' action type
        if !["Delete", "Evict"].contains(&self.then.action_type.as_str()) {
            return Err(format!(
                "unsupported action type '{}': must be 'Delete' or 'Evict'",
                self.then.action_type
            ));
        }

        // Validate grace period if present
        if let Some(grace) = self.then.grace_period_seconds {
            if grace < 0 {
                return Err("then.gracePeriodSeconds must be non-negative".to_string());
            }
        }

        Ok(())
    }
}
