use crate::Result;
use cel::{Context, Program, Value};
use chrono::Utc;
use dashmap::DashMap;
use k8s_openapi::api::core::v1::Pod;
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

    /// Compile and evaluate a CEL expression
    /// Note: Now takes &self instead of &mut self for non-blocking concurrent evaluation
    pub fn evaluate(&self, expr: &str, pod: &Pod) -> Result<bool> {
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
                    warn!("CEL compilation error for expression '{}': {}", expr, e);
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
                        warn!(
                            "CEL expression '{}' did not evaluate to boolean, got: {:?}",
                            expr, result
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
                warn!("CEL evaluation error for expression '{}': {}", expr, e);
                Err(crate::Error::CelEvaluationError(e.to_string()))
            }
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
/// Injects the entire Pod object for full CEL expression support
fn build_evaluation_context(pod: &Pod) -> Result<Context<'_>> {
    let mut context = Context::default();

    // Convert Pod to serde_json::Value then to cel::Value
    let pod_json = json_to_value(pod)?;
    let cel_pod_value = cel::to_value(&pod_json)
        .map_err(|e| crate::Error::CelEvaluationError(e.to_string()))?;

    // Inject Pod object under multiple variable names for compatibility
    // "object" is the standard CEL variable name for the evaluated resource
    let _ = context.add_variable("object", cel_pod_value.clone());
    // "self" is used in examples/cel-policy.yaml for backward compatibility
    let _ = context.add_variable("self", cel_pod_value.clone());
    
    // "status" shortcut for direct access to status subfield
    if let Some(status) = &pod.status {
        if let Ok(status_json) = json_to_value(status) {
            if let Ok(status_cel) = cel::to_value(&status_json) {
                let _ = context.add_variable("status", status_cel);
            }
        }
    }
    
    // "metadata" shortcut for direct access to metadata subfield
    if let Ok(metadata_json) = json_to_value(&pod.metadata) {
        if let Ok(metadata_cel) = cel::to_value(&metadata_json) {
            let _ = context.add_variable("metadata", metadata_cel);
        }
    }

    // Add convenient shortcuts for common queries
    let creation_timestamp = pod
        .metadata
        .creation_timestamp
        .as_ref()
        .map(|t| t.0);

    if let Some(created) = creation_timestamp {
        let now = Utc::now();
        let age_seconds = (now - created).num_seconds();
        let _ = context.add_variable("age", Value::Int(age_seconds));
        let _ = context.add_variable("now", Value::Int(now.timestamp()));
    }

    // Add namespace shortcut
    if let Some(ns) = pod.metadata.namespace.as_ref() {
        let _ = context.add_variable("namespace", Value::String(Arc::new(ns.clone())));
    }

    // Add Pod name shortcut
    if let Some(name) = pod.metadata.name.as_ref() {
        let _ = context.add_variable("name", Value::String(Arc::new(name.clone())));
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

        let _result = evaluator.evaluate("age > 600", &pod);
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

        let result = evaluator.evaluate("age > 600", &pod);
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

        let result = evaluator.evaluate("age < 200", &pod);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_pod_status_access() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        pod.status = Some(k8s_openapi::api::core::v1::PodStatus {
            phase: Some("Failed".to_string()),
            ..Default::default()
        });

        // Test accessing via status shortcut
        let result = evaluator.evaluate("status.phase == 'Failed'", &pod);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_self_reference() {
        let evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        pod.status = Some(k8s_openapi::api::core::v1::PodStatus {
            phase: Some("Failed".to_string()),
            ..Default::default()
        });

        // Test accessing via self reference (backward compatibility)
        let result = evaluator.evaluate("self.status.phase == 'Failed'", &pod);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_invalid_expression() {
        let evaluator = CelEvaluator::new();
        let pod = Pod::default();

        let result = evaluator.evaluate("this is not valid cel !!!!", &pod);
        assert!(result.is_err());
    }

    #[test]
    fn test_compilation_error() {
        let evaluator = CelEvaluator::new();
        let pod = Pod::default();

        let result = evaluator.evaluate("age >", &pod);
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
        let _result1 = evaluator.evaluate(expr, &pod);
        assert_eq!(evaluator.cache_size(), 1);

        // Second evaluation - should use cache
        let _result2 = evaluator.evaluate(expr, &pod);
        assert_eq!(evaluator.cache_size(), 1);
    }
}
