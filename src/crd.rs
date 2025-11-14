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
    /// Validate the spec
    pub fn validate(&self) -> Result<(), String> {
        if self.trigger.annotation_key.is_empty() {
            return Err("trigger.annotation_key cannot be empty".to_string());
        }

        if self.trigger.annotation_values.is_empty() {
            return Err("trigger.annotation_values cannot be empty".to_string());
        }

        if self.when.condition_type == "CEL" && self.when.expression.is_none() {
            return Err("when.expression required for CEL type".to_string());
        }

        if self.when.condition_type == "Builtin" && self.when.ttl_seconds.is_none() {
            return Err("when.ttl_seconds required for Builtin type".to_string());
        }

        if !["Delete", "Evict"].contains(&self.then.action_type.as_str()) {
            return Err(format!(
                "unsupported action type: {}",
                self.then.action_type
            ));
        }

        Ok(())
    }
}
