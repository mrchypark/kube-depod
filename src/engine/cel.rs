use crate::Result;
use cel::{Context, Program, Value};
use chrono::Utc;
use k8s_openapi::api::core::v1::Pod;
use std::sync::Arc;
use tracing::{debug, warn};

/// CEL expression evaluator with caching
pub struct CelEvaluator {
    /// Cache of compiled CEL programs
    expression_cache: std::collections::HashMap<String, Arc<Program>>,
}

impl CelEvaluator {
    pub fn new() -> Self {
        Self {
            expression_cache: std::collections::HashMap::new(),
        }
    }

    /// Compile and evaluate a CEL expression
    pub fn evaluate(&mut self, expr: &str, pod: &Pod) -> Result<bool> {
        // Get or compile the expression
        let program = if let Some(cached) = self.expression_cache.get(expr) {
            cached.clone()
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

        // Evaluate
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
    pub fn clear_cache(&mut self) {
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
fn build_evaluation_context(pod: &Pod) -> Result<Context> {
    let mut context = Context::default();

    // Add convenient shortcuts
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

    // Add namespace
    if let Some(ns) = pod.metadata.namespace.as_ref() {
        let _ = context.add_variable("namespace", Value::String(Arc::new(ns.clone())));
    }

    // Add Pod name
    if let Some(name) = pod.metadata.name.as_ref() {
        let _ = context.add_variable("name", Value::String(Arc::new(name.clone())));
    }

    // Add phase
    if let Some(status) = &pod.status {
        if let Some(phase) = &status.phase {
            let _ = context.add_variable("phase", Value::String(Arc::new(phase.clone())));
        }

        // Add restart count
        if let Some(container_statuses) = &status.container_statuses {
            if !container_statuses.is_empty() {
                let restart_count = container_statuses[0].restart_count as i64;
                let _ = context.add_variable("restartCount", Value::Int(restart_count));
            }
        }
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
        let mut evaluator = CelEvaluator::new();
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
        let mut evaluator = CelEvaluator::new();
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
        let mut evaluator = CelEvaluator::new();
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
    fn test_phase_comparison() {
        let mut evaluator = CelEvaluator::new();
        let mut pod = Pod::default();
        pod.status = Some(k8s_openapi::api::core::v1::PodStatus {
            phase: Some("Failed".to_string()),
            ..Default::default()
        });

        let result = evaluator.evaluate("phase == 'Failed'", &pod);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_invalid_expression() {
        let mut evaluator = CelEvaluator::new();
        let pod = Pod::default();

        let result = evaluator.evaluate("this is not valid cel !!!!", &pod);
        assert!(result.is_err());
    }

    #[test]
    fn test_compilation_error() {
        let mut evaluator = CelEvaluator::new();
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
        let mut evaluator = CelEvaluator::new();
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
