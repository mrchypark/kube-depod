use crate::Result;
use cel::{Context, Program, Value};
use chrono::Utc;
use dashmap::DashMap;
use k8s_openapi::api::core::v1::Pod;
use kube::ResourceExt;
use serde_json::to_value as json_to_value;
use std::sync::Arc;
use tracing::{debug, warn};

/// CEL expression evaluator with concurrent caching
pub struct CelEvaluator {
    /// Cache of compiled CEL programs (concurrent, lock-free)
    expression_cache: DashMap<String, Arc<Program>>,
}

impl CelEvaluator {
    pub fn new() -> Self {
        Self {
            expression_cache: DashMap::new(),
        }
    }

    /// Compile and evaluate a CEL expression with policy/pod context
    ///
    /// Parameters:
    /// - expr: CEL expression to evaluate
    /// - pod: Pod object to evaluate against
    /// - policy_name: Name of the policy (for logging context)
    ///
    /// Note: Takes &self instead of &mut self for non-blocking concurrent evaluation
    pub fn evaluate(
        &self,
        expr: &str,
        pod: &Pod,
        policy_name: &str,
    ) -> Result<bool> {
        // Get or compile the expression
        let program = if let Some(cached) = self.expression_cache.get(expr) {
            cached.value().clone()
        } else {
            // Compile new expression
            match Program::compile(expr) {
                Ok(prog) => {
                    let prog = Arc::new(prog);
                    self.expression_cache.insert(expr.to_string(), prog.clone());
                    prog
                }
                Err(e) => {
                    let pod_ns = pod.namespace().unwrap_or_default();
                    let pod_name = pod.name_any();
                    warn!(
                        "CEL compilation error for policy={} pod={}/{} expr='{}': {}",
                        policy_name, pod_ns, pod_name, expr, e
                    );
                    return Err(crate::Error::CelCompilationError(e.to_string()));
                }
            }
        };

        // Create evaluation context
        let context = build_evaluation_context(pod)?;

        // Evaluate (no lock held during expensive operation)
        match program.execute(&context) {
            Ok(result) => {
                let bool_result = match result {
                    Value::Bool(b) => b,
                    Value::Int(i) => i != 0,
                    Value::UInt(u) => u != 0,
                    Value::Float(f) => f != 0.0,
                    _ => {
                        let pod_ns = pod.namespace().unwrap_or_default();
                        let pod_name = pod.name_any();
                        warn!(
                            "CEL non-boolean result for policy={} pod={}/{} expr='{}', got: {:?}",
                            policy_name, pod_ns, pod_name, expr, result
                        );
                        return Err(crate::Error::CelEvaluationError(
                            "Expression did not evaluate to boolean".to_string(),
                        ));
                    }
                };
                debug!("CEL evaluation '{}' = {}", expr, bool_result);
                Ok(bool_result)
            }
            Err(e) => {
                let pod_ns = pod.namespace().unwrap_or_default();
                let pod_name = pod.name_any();
                warn!(
                    "CEL evaluation error for policy={} pod={}/{} expr='{}': {}",
                    policy_name, pod_ns, pod_name, expr, e
                );
                Err(crate::Error::CelEvaluationError(e.to_string()))
            }
        }
    }

    /// Validate a CEL expression by compiling it
    pub fn validate(&self, expr: &str) -> Result<()> {
        if self.expression_cache.contains_key(expr) {
            return Ok(());
        }
        match Program::compile(expr) {
            Ok(prog) => {
                let prog = Arc::new(prog);
                self.expression_cache.insert(expr.to_string(), prog);
                Ok(())
            }
            Err(e) => Err(crate::Error::CelCompilationError(e.to_string())),
        }
    }

    /// Clear the expression cache
    pub fn clear_cache(&self) {
        self.expression_cache.clear();
    }

    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.expression_cache.len()
    }
}

impl Default for CelEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Build CEL context from a Pod
/// Provides a clean, consistent set of CEL variables for policy evaluation:
/// - pod: full Pod object (root variable)
/// - metadata/spec/status: shortcut accessors for Pod fields
/// - now: current timestamp (epoch seconds, UTC)
/// - age: seconds since creationTimestamp
fn build_evaluation_context(pod: &Pod) -> Result<Context<'_>> {
    let mut context = Context::default();

    // (A) Inject the entire Pod object as "pod" (root variable)
    let pod_json = json_to_value(pod)?;
    let cel_pod_value = cel::to_value(&pod_json)
        .map_err(|e| crate::Error::CelEvaluationError(e.to_string()))?;
    let _ = context.add_variable("pod", cel_pod_value);

    // (B) Add shortcut accessors for Pod root fields
    
    // "metadata" shortcut
    if let Ok(metadata_json) = json_to_value(&pod.metadata) {
        if let Ok(metadata_cel) = cel::to_value(&metadata_json) {
            let _ = context.add_variable("metadata", metadata_cel);
        }
    }

    // "spec" shortcut
    if let Some(spec) = &pod.spec {
        if let Ok(spec_json) = json_to_value(spec) {
            if let Ok(spec_cel) = cel::to_value(&spec_json) {
                let _ = context.add_variable("spec", spec_cel);
            }
        }
    }

    // "status" shortcut
    if let Some(status) = &pod.status {
        if let Ok(status_json) = json_to_value(status) {
            if let Ok(status_cel) = cel::to_value(&status_json) {
                let _ = context.add_variable("status", status_cel);
            }
        }
    } else {
        // If status is missing, provide empty object to avoid "undeclared reference" error
        // This allows expressions like 'has(status.phase)' to work (evaluates to false)
        // or 'status.phase' (evaluates to error/null depending on CEL config)
        if let Ok(empty_obj) = cel::to_value(serde_json::json!({})) {
            let _ = context.add_variable("status", empty_obj);
        }
    }

    // (C) Add time variables
    
    // "now": current timestamp in epoch seconds (UTC)
    let now = Utc::now().timestamp();
    let _ = context.add_variable("now", Value::Int(now));

    // "age": seconds since creationTimestamp
    if let Some(created) = pod.metadata.creation_timestamp.as_ref() {
        let created_ts = created.0.timestamp();
        let mut age = now - created_ts;
        if age < 0 {
            age = 0; // Protect against clock skew
        }
        let _ = context.add_variable("age", Value::Int(age));
    }

    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluator_new() {
        let evaluator = CelEvaluator::new();
        assert_eq!(evaluator.cache_size(), 0);
    }

    #[test]
    fn test_evaluator_clears_cache() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(700);
        pod.metadata.creation_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(past));

        let _result = evaluator.evaluate("age > 600", &pod, "test-policy");
        assert!(evaluator.cache_size() > 0);

        evaluator.clear_cache();
        assert_eq!(evaluator.cache_size(), 0);
    }

    #[test]
    fn test_simple_integer_comparison() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(700);
        pod.metadata.creation_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(past));

        let result = evaluator.evaluate("age > 600", &pod, "test-policy");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_pod_age_less_than() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(100);
        pod.metadata.creation_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(past));

        let result = evaluator.evaluate("age < 200", &pod, "test-policy");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_pod_status_access() {
        let evaluator = CelEvaluator::new();
        let pod = Pod {
            status: Some(k8s_openapi::api::core::v1::PodStatus {
                phase: Some("Failed".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Test accessing via status shortcut
        let result = evaluator.evaluate("status.phase == 'Failed'", &pod, "test-policy");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_pod_reference() {
        let evaluator = CelEvaluator::new();
        let pod = Pod {
            status: Some(k8s_openapi::api::core::v1::PodStatus {
                phase: Some("Failed".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Test accessing via pod root variable
        let result = evaluator.evaluate("pod.status.phase == 'Failed'", &pod, "test-policy");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_invalid_expression() {
        let evaluator = CelEvaluator::new();
        let pod = Pod::default();

        let result = evaluator.evaluate("this is not valid cel !!!!", &pod, "test-policy");
        assert!(result.is_err());
    }

    #[test]
    fn test_compilation_error() {
        let evaluator = CelEvaluator::new();
        let pod = Pod::default();

        let result = evaluator.evaluate("age >", &pod, "test-policy");
        assert!(result.is_err());
        if let Err(crate::Error::CelCompilationError(_)) = result {
            // Expected
        } else {
            panic!("Expected CelCompilationError");
        }
    }

    #[test]
    fn test_cache_hit() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(700);
        pod.metadata.creation_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(past));

        let expr = "age > 600";

        // First evaluation - compilation happens
        let _result1 = evaluator.evaluate(expr, &pod, "test-policy");
        assert_eq!(evaluator.cache_size(), 1);

        // Second evaluation - should use cache
        let _result2 = evaluator.evaluate(expr, &pod, "test-policy");
        assert_eq!(evaluator.cache_size(), 1);
    }
}
