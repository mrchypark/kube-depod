use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Policy CRD for automated Pod cleanup
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "kube-depod.io",
    version = "v1alpha1",
    kind = "Policy",
    namespaced
)]
pub struct PolicySpec {
    /// Target namespace and pod selectors
    pub target: Target,

    /// Trigger condition (annotation-based)
    pub trigger: Trigger,

    /// Condition expression for evaluation
    pub condition: Condition,

    /// Action to perform
    pub action: Action,

    /// Safety limits
    pub limits: Limits,
}

/// Target specifies which pods are affected by this policy
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Target {
    /// Namespace selector (matchNames or matchExpressions)
    #[serde(default)]
    pub namespace_selector: Option<NamespaceSelector>,

    /// Pod label selector
    #[serde(default)]
    pub pod_selector: Option<PodSelector>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct NamespaceSelector {
    /// Match specific namespace names
    #[serde(default)]
    pub match_names: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct PodSelector {
    /// Match pods by labels
    #[serde(default)]
    pub match_labels: BTreeMap<String, String>,
}

/// Trigger specifies when the policy should be checked
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Trigger {
    /// Annotation key to look for
    pub annotation_key: String,

    /// Accepted annotation values
    pub annotation_values: Vec<String>,
}

/// Condition specifies the evaluation rule
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Condition {
    /// Type of condition: CEL or Builtin
    #[serde(rename = "type")]
    pub condition_type: String,

    /// CEL expression or condition parameters
    #[serde(default)]
    pub expression: Option<String>,

    /// TTL seconds for builtin TTL type
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
}

/// Action specifies what to do when condition is met
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Action {
    /// Type of action: Delete, Evict, etc.
    #[serde(rename = "type")]
    pub action_type: String,

    /// Grace period for deletion
    #[serde(default)]
    pub grace_period_seconds: Option<i64>,

    /// Dry run mode (don't actually delete)
    #[serde(default)]
    pub dry_run: bool,
}

/// Limits defines safety guardrails
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Limits {
    /// Max pods to delete per minute
    #[serde(default)]
    pub max_deletes_per_minute: Option<i32>,

    /// Protect system namespaces
    #[serde(default)]
    pub protect_system_namespaces: bool,
}

impl PolicySpec {
    /// Validate the spec
    pub fn validate(&self) -> Result<(), String> {
        if self.trigger.annotation_key.is_empty() {
            return Err("trigger.annotation_key cannot be empty".to_string());
        }

        if self.trigger.annotation_values.is_empty() {
            return Err("trigger.annotation_values cannot be empty".to_string());
        }

        if self.condition.condition_type == "CEL" && self.condition.expression.is_none() {
            return Err("condition.expression required for CEL type".to_string());
        }

        if self.condition.condition_type == "Builtin" && self.condition.ttl_seconds.is_none() {
            return Err("condition.ttl_seconds required for Builtin type".to_string());
        }

        if !["Delete", "Evict"].contains(&self.action.action_type.as_str()) {
            return Err(format!(
                "unsupported action type: {}",
                self.action.action_type
            ));
        }

        Ok(())
    }
}
