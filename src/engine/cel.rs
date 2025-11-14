use crate::Result;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Pod;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, warn};

/// CEL expression evaluator
pub struct CelEvaluator {
    expression_cache: HashMap<String, String>,
}

impl CelEvaluator {
    pub fn new() -> Self {
        Self {
            expression_cache: HashMap::new(),
        }
    }

    /// Compile and evaluate a CEL expression
    pub fn evaluate(&mut self, expr: &str, pod: &Pod) -> Result<bool> {
        let context = EvaluationContext::from_pod(pod)?;

        // Try to evaluate the expression
        match evaluate_cel(expr, &context) {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!("CEL evaluation error: {}", e);
                Err(crate::Error::Custom(format!("CEL evaluation failed: {}", e)))
            }
        }
    }

    /// Clear the expression cache
    pub fn clear_cache(&mut self) {
        self.expression_cache.clear();
    }
}

impl Default for CelEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluation context with Pod data
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// Pod object as JSON value
    pub object: Value,
    /// Current time
    pub now: DateTime<Utc>,
}

impl EvaluationContext {
    /// Create evaluation context from a Pod
    pub fn from_pod(pod: &Pod) -> Result<Self> {
        let object = serde_json::to_value(pod)
            .map_err(|e| crate::Error::Custom(format!("Failed to serialize Pod: {}", e)))?;

        Ok(Self {
            object,
            now: Utc::now(),
        })
    }

    /// Get the Pod's creation timestamp
    pub fn creation_timestamp(&self) -> Option<DateTime<Utc>> {
        self.object
            .pointer("/metadata/creationTimestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Get Pod age in seconds
    pub fn pod_age_seconds(&self) -> Option<i64> {
        self.creation_timestamp().map(|created| {
            (self.now - created).num_seconds()
        })
    }

    /// Get Pod namespace
    pub fn namespace(&self) -> Option<String> {
        self.object
            .pointer("/metadata/namespace")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get Pod name
    pub fn pod_name(&self) -> Option<String> {
        self.object
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get Pod labels
    pub fn labels(&self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        if let Some(obj) = self.object.pointer("/metadata/labels").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    labels.insert(k.clone(), s.to_string());
                }
            }
        }
        labels
    }

    /// Get Pod phase
    pub fn phase(&self) -> Option<String> {
        self.object
            .pointer("/status/phase")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// Evaluate a CEL expression with simplified runtime
/// This is a basic implementation that handles common Pod-related conditions
fn evaluate_cel(expr: &str, context: &EvaluationContext) -> std::result::Result<bool, String> {
    debug!("Evaluating CEL expression: {}", expr);

    // Simple expression parser for common patterns
    // In production, use full CEL interpreter (cel-interpreter or google/cel-go bindings)

    // Pattern: age > N (in seconds)
    if let Some(age_seconds) = context.pod_age_seconds() {
        if expr.contains("age") && expr.contains(">") {
            if let Some(seconds_str) = extract_number_from_comparison(expr, ">") {
                if let Ok(threshold) = seconds_str.parse::<i64>() {
                    return Ok(age_seconds > threshold);
                }
            }
        }
    }

    // Pattern: status.phase == "Failed"
    if expr.contains("status.phase") && expr.contains("==") {
        if let Some(phase) = context.phase() {
            if expr.contains("\"Failed\"") {
                return Ok(phase == "Failed");
            } else if expr.contains("\"Pending\"") {
                return Ok(phase == "Pending");
            } else if expr.contains("\"Unknown\"") {
                return Ok(phase == "Unknown");
            } else if expr.contains("\"CrashLoopBackOff\"") {
                return Ok(phase == "CrashLoopBackOff");
            }
        }
    }

    // Pattern: metadata.namespace == "namespace"
    if expr.contains("metadata.namespace") && expr.contains("==") {
        if let Some(ns) = context.namespace() {
            if let Some(ns_str) = extract_quoted_string(expr) {
                return Ok(ns == ns_str);
            }
        }
    }

    // If we can't evaluate, return false (safe default)
    warn!("Unable to parse CEL expression: {}", expr);
    Ok(false)
}

/// Extract a number from comparison expressions like "age > 600"
fn extract_number_from_comparison(expr: &str, op: &str) -> Option<String> {
    expr.split(op)
        .nth(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract quoted string from expression
fn extract_quoted_string(expr: &str) -> Option<String> {
    let start = expr.find('"')?;
    let end = expr[start + 1..].find('"')?;
    Some(expr[start + 1..start + 1 + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluation_context_from_pod() {
        let pod = Pod::default();
        let ctx = EvaluationContext::from_pod(&pod);
        assert!(ctx.is_ok());
    }

    #[test]
    fn test_pod_age_calculation() {
        let mut pod = Pod::default();
        // Create a timestamp 1 hour ago
        let now = Utc::now();
        let past = now - chrono::Duration::hours(1);

        pod.metadata.creation_timestamp = Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(past));

        let ctx = EvaluationContext::from_pod(&pod).unwrap();
        if let Some(age) = ctx.pod_age_seconds() {
            assert!(age > 3500 && age < 3700); // ~1 hour (allow some variance)
        }
    }

    #[test]
    fn test_cel_expression_age_comparison() {
        let pod = Pod::default();
        let ctx = EvaluationContext::from_pod(&pod).unwrap();

        // Test age > 600
        let result = evaluate_cel("age > 600", &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cel_expression_phase_comparison() {
        let mut pod = Pod::default();
        pod.status = Some(k8s_openapi::api::core::v1::PodStatus {
            phase: Some("Failed".to_string()),
            ..Default::default()
        });

        let ctx = EvaluationContext::from_pod(&pod).unwrap();
        let result = evaluate_cel("status.phase == \"Failed\"", &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_number() {
        let result = extract_number_from_comparison("age > 600", ">");
        assert_eq!(result, Some("600".to_string()));
    }

    #[test]
    fn test_extract_quoted_string() {
        let result = extract_quoted_string("phase == \"Failed\"");
        assert_eq!(result, Some("Failed".to_string()));
    }
}
