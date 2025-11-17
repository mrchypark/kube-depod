//! Integration tests for CEL expression evaluation
//! Tests the CEL engine against real-world policy expressions from examples/cel-policy.yaml

use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStateWaiting, ContainerStatus, Pod, PodStatus, PodCondition,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Time, ObjectMeta};
use kube_depod::engine::CelEvaluator;
use chrono::Utc;

/// Create a Pod with the given status phase
fn create_pod_with_phase(namespace: &str, name: &str, phase: &str) -> Pod {
    let mut pod = Pod::default();
    pod.metadata = ObjectMeta {
        name: Some(name.to_string()),
        namespace: Some(namespace.to_string()),
        ..Default::default()
    };
    pod.status = Some(PodStatus {
        phase: Some(phase.to_string()),
        ..Default::default()
    });
    pod
}

/// Create a Pod with CrashLoopBackOff status
fn create_crashloop_pod(namespace: &str, name: &str, restart_count: i32) -> Pod {
    let mut pod = Pod::default();
    pod.metadata = ObjectMeta {
        name: Some(name.to_string()),
        namespace: Some(namespace.to_string()),
        ..Default::default()
    };
    pod.status = Some(PodStatus {
        phase: Some("Running".to_string()),
        container_statuses: Some(vec![ContainerStatus {
            name: "app".to_string(),
            ready: false,
            restart_count,
            state: Some(ContainerState {
                waiting: Some(ContainerStateWaiting {
                    reason: Some("CrashLoopBackOff".to_string()),
                    message: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        }]),
        ..Default::default()
    });
    pod
}

/// Create a Pod with ImagePullBackOff status
fn create_imagepull_backoff_pod(namespace: &str, name: &str, reason: &str) -> Pod {
    let mut pod = Pod::default();
    pod.metadata = ObjectMeta {
        name: Some(name.to_string()),
        namespace: Some(namespace.to_string()),
        ..Default::default()
    };
    pod.status = Some(PodStatus {
        phase: Some("Pending".to_string()),
        container_statuses: Some(vec![ContainerStatus {
            name: "app".to_string(),
            ready: false,
            restart_count: 0,
            state: Some(ContainerState {
                waiting: Some(ContainerStateWaiting {
                    reason: Some(reason.to_string()),
                    message: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        }]),
        ..Default::default()
    });
    pod
}



/// Create a Pod with a specific age (creation timestamp)
fn create_pod_with_age(namespace: &str, name: &str, age_seconds: i64) -> Pod {
    let mut pod = Pod::default();
    let now = Utc::now();
    let past = now - chrono::Duration::seconds(age_seconds);
    pod.metadata = ObjectMeta {
        name: Some(name.to_string()),
        namespace: Some(namespace.to_string()),
        creation_timestamp: Some(Time(past)),
        ..Default::default()
    };
    pod.status = Some(PodStatus::default());
    pod
}

#[test]
fn test_cel_crashloop_detection() {
    let evaluator = CelEvaluator::new();
    let pod = create_crashloop_pod("default", "crashloop-app", 5);

    // Expression from examples/cel-policy.yaml - Policy 2
    let expr = r#"status.containerStatuses.exists(c,
        has(c.state.waiting) &&
        c.state.waiting.reason == 'CrashLoopBackOff' &&
        c.restartCount >= 5
    )"#;

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok(), "CEL evaluation failed: {:?}", result);
    assert!(result.unwrap(), "Should detect CrashLoopBackOff pod");
}

#[test]
fn test_cel_image_pull_backoff_detection() {
    let evaluator = CelEvaluator::new();

    // Test ImagePullBackOff
    let pod = create_imagepull_backoff_pod("default", "image-pull-failed", "ImagePullBackOff");

    // Expression from examples/cel-policy.yaml - Policy 4
    let expr = r#"status.containerStatuses.exists(c,
        has(c.state.waiting) && (
            c.state.waiting.reason == 'ImagePullBackOff' ||
            c.state.waiting.reason == 'ErrImagePull'
        )
    )"#;

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect ImagePullBackOff pod");

    // Test ErrImagePull
    let pod2 = create_imagepull_backoff_pod("default", "image-err", "ErrImagePull");
    let result2 = evaluator.evaluate(expr, &pod2, "test-policy");
    assert!(result2.is_ok());
    assert!(result2.unwrap(), "Should detect ErrImagePull pod");
}

#[test]
fn test_cel_succeeded_phase_detection() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "batch-job", "Succeeded");

    // Expression from examples/cel-policy.yaml - Policy 7
    let expr = "status.phase == 'Succeeded'";

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect Succeeded pod");
}

#[test]
fn test_cel_failed_phase_detection() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "failed-job", "Failed");

    let expr = "status.phase == 'Failed'";

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect Failed pod");
}

#[test]
fn test_cel_pod_age_threshold() {
    let evaluator = CelEvaluator::new();

    // Pod older than 30 minutes
    let old_pod = create_pod_with_age("default", "old-pod", 1800 + 100);

    let expr = "age > 1800";
    let result = evaluator.evaluate(expr, &old_pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect old pod (>30min)");

    // Pod younger than 30 minutes
    let young_pod = create_pod_with_age("default", "young-pod", 600);
    let result = evaluator.evaluate(expr, &young_pod, "test-policy");
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Should not match young pod (<30min)");
}

#[test]
fn test_cel_high_restart_count() {
    let evaluator = CelEvaluator::new();
    let pod = create_crashloop_pod("default", "flaky-app", 15);

    // Expression from examples/cel-policy.yaml - Policy 6
    let expr = r#"status.containerStatuses.exists(c,
        c.restartCount > 10 &&
        c.ready == false
    )"#;

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect pod with high restart count");
}

#[test]
fn test_cel_complex_ready_condition_check() {
    let evaluator = CelEvaluator::new();

    let terminated_status = ContainerStatus {
        name: "app".to_string(),
        ready: false,
        restart_count: 2,
        state: Some(ContainerState {
            terminated: Some(k8s_openapi::api::core::v1::ContainerStateTerminated {
                exit_code: 0,
                reason: Some("Completed".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut pod = Pod::default();
    pod.metadata = ObjectMeta {
        name: Some("completed-pod".to_string()),
        namespace: Some("default".to_string()),
        ..Default::default()
    };
    pod.status = Some(PodStatus {
        phase: Some("Failed".to_string()),
        container_statuses: Some(vec![terminated_status]),
        conditions: Some(vec![PodCondition {
            type_: "Ready".to_string(),
            status: "False".to_string(),
            last_probe_time: None,
            last_transition_time: None,
            reason: None,
            message: None,
        }]),
        ..Default::default()
    });

    // Expression - Policy for ready condition and terminated state
    let expr = r#"(
        has(pod.status.conditions) &&
        pod.status.conditions.exists(cond,
            cond.type == 'Ready' && cond.status == 'False'
        )
    ) &&
    (
        has(pod.status.containerStatuses) &&
        pod.status.containerStatuses.exists(c,
            c.ready == false &&
            c.restartCount > 0 &&
            has(c.state.terminated) &&
            c.state.terminated.reason == 'Completed'
        )
    )"#;

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok(), "CEL evaluation failed: {:?}", result);
    assert!(result.unwrap(), "Should detect completed pod with Failed status");
}

#[test]
fn test_cel_multi_condition_or_logic() {
    let evaluator = CelEvaluator::new();

    // Pod with CrashLoopBackOff in one container
    let pod = create_crashloop_pod("default", "multi-error-pod", 3);

    // Expression from examples/cel-policy.yaml - Policy 8
    let expr = r#"(
        status.containerStatuses.exists(c,
            has(c.state.waiting) && (
                c.state.waiting.reason == 'CrashLoopBackOff' ||
                c.state.waiting.reason == 'ImagePullBackOff'
            )
        )
    ) &&
    (
        status.containerStatuses.all(c, c.ready == false)
    )"#;

    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should match CrashLoopBackOff OR ImagePullBackOff AND all not ready");
}

#[test]
fn test_cel_pod_reference() {
let evaluator = CelEvaluator::new();
let pod = create_pod_with_phase("default", "test-pod", "Pending");

// Test "pod" root variable access
let expr_pod = "pod.status.phase == 'Pending'";
let result_pod = evaluator.evaluate(expr_pod, &pod, "test-policy");
assert!(result_pod.is_ok());
assert!(result_pod.unwrap());
}

#[test]
fn test_cel_metadata_namespace_access() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("production", "prod-pod", "Running");

    let expr = "metadata.namespace == 'production'";
    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_cel_metadata_name_access() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "my-app-123", "Running");

    let expr = "metadata.name == 'my-app-123'";
    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_cel_expression_caching() {
    let evaluator = CelEvaluator::new();
    let pod = create_crashloop_pod("default", "app", 5);
    
    let expr = "status.containerStatuses.exists(c, c.restartCount > 3)";

    // First evaluation - compiles expression
    let result1 = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result1.is_ok());
    let cache_size_1 = evaluator.cache_size();
    assert_eq!(cache_size_1, 1, "Cache should contain 1 compiled expression");

    // Second evaluation - uses cached expression
    let result2 = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result2.is_ok());
    let cache_size_2 = evaluator.cache_size();
    assert_eq!(cache_size_2, 1, "Cache size should remain 1");

    // Different expression
    let expr2 = "status.phase == 'Running'";
    let result3 = evaluator.evaluate(expr2, &pod, "test-policy");
    assert!(result3.is_ok());
    let cache_size_3 = evaluator.cache_size();
    assert_eq!(cache_size_3, 2, "Cache should now contain 2 expressions");
}

#[test]
fn test_cel_invalid_expression_error() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "test", "Running");

    // Invalid CEL syntax
    let invalid_expr = "this is not valid cel syntax !!!";
    let result = evaluator.evaluate(invalid_expr, &pod, "test-policy");
    assert!(result.is_err(), "Should fail on invalid expression");
}

#[test]
fn test_cel_syntax_error_compilation() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "test", "Running");

    // Incomplete expression
    let invalid_expr = "status.phase ==";
    let result = evaluator.evaluate(invalid_expr, &pod, "test-policy");
    assert!(result.is_err(), "Should fail on syntax error");
}

#[test]
fn test_cel_no_container_statuses() {
    let evaluator = CelEvaluator::new();
    let pod = create_pod_with_phase("default", "empty-pod", "Unknown");

    // Expression should safely handle missing containerStatuses
    let expr = "has(status.containerStatuses) && status.containerStatuses.size() > 0";
    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Should return false when no container statuses");
}

#[test]
fn test_has_status() {
    let evaluator = CelEvaluator::new();
    let mut pod = Pod::default();
    pod.status = Some(PodStatus {
        phase: Some("Succeeded".to_string()),
        ..Default::default()
    });

    // Test has(status.phase) && status.phase == 'Succeeded'
    let result = evaluator.evaluate("has(status.phase) && status.phase == 'Succeeded'", &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap(), "Should detect Succeeded phase");
}

#[test]
fn test_cel_metadata_access() {
    let evaluator = CelEvaluator::new();
    let mut pod = Pod::default();
    pod.metadata = ObjectMeta {
        name: Some("test-pod".to_string()),
        namespace: Some("test-ns".to_string()),
        ..Default::default()
    };

    // Access via metadata shortcut
    let expr = "metadata.name == 'test-pod'";
    let result = evaluator.evaluate(expr, &pod, "test-policy");
    assert!(result.is_ok());
    assert!(result.unwrap());

    // Access via pod.metadata
    let expr2 = "pod.metadata.namespace == 'test-ns'";
    let result2 = evaluator.evaluate(expr2, &pod, "test-policy");
    assert!(result2.is_ok());
    assert!(result2.unwrap());
}
