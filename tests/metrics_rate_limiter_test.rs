use kube_depod::metrics::Metrics;
use kube_depod::rate_limiter::RateLimiter;
use std::sync::Arc;

#[test]
fn test_metrics_with_rate_limiter() {
    // Create metrics and rate limiter
    let metrics = Arc::new(Metrics::new());
    let rate_limiter = Arc::new(RateLimiter::new(3)); // Allow 3 per minute

    // Test metrics increment
    metrics.increment_pods_evaluated();
    metrics.increment_pods_evaluated();
    assert_eq!(metrics.get_pods_evaluated(), 2);

    // Test rate limiter
    assert!(rate_limiter.allow()); // First request allowed
    assert!(rate_limiter.allow()); // Second request allowed
    assert!(rate_limiter.allow()); // Third request allowed
    assert!(!rate_limiter.allow()); // Fourth request denied

    // Record rate limit hit
    metrics.increment_rate_limited();
    assert_eq!(metrics.get_rate_limited(), 1);

    // Test policy matches
    metrics.increment_policy_matches();
    metrics.increment_policy_matches();
    assert_eq!(metrics.get_policy_matches(), 2);

    // Test pod deleted
    metrics.increment_pods_deleted();
    assert_eq!(metrics.get_pods_deleted(), 1);

    // Test evaluation errors
    metrics.increment_evaluation_errors();
    assert_eq!(metrics.get_evaluation_errors(), 1);

    // Verify prometheus format contains all metrics
    let prometheus_output = metrics.prometheus_format();
    assert!(prometheus_output.contains("kube_depod_pods_evaluated_total {} 2"));
    assert!(prometheus_output.contains("kube_depod_pods_deleted_total {} 1"));
    assert!(prometheus_output.contains("kube_depod_policy_matches_total {} 2"));
    assert!(prometheus_output.contains("kube_depod_evaluation_errors_total {} 1"));
    assert!(prometheus_output.contains("kube_depod_rate_limited_total {} 1"));
}

#[test]
fn test_rate_limiter_recovery() {
    let rate_limiter = Arc::new(RateLimiter::new(2));

    // Exhaust the limit
    assert!(rate_limiter.allow());
    assert!(rate_limiter.allow());
    assert!(!rate_limiter.allow());

    // Tokens should be 0 after exhaustion
    assert_eq!(rate_limiter.get_tokens(), 0);

    // After minute passes (not testing actual time), limit should stay same
    // This is just a sanity check that the limit remains
    assert_eq!(rate_limiter.get_max_per_minute(), 2);
}

#[test]
fn test_concurrent_metrics_increment() {
    let metrics = Arc::new(Metrics::new());
    let mut handles = vec![];

    // Spawn 10 threads that each increment metrics
    for _ in 0..10 {
        let m = metrics.clone();
        let handle = std::thread::spawn(move || {
            m.increment_pods_evaluated();
            m.increment_policy_matches();
            m.increment_pods_deleted();
            m.increment_evaluation_errors();
            m.increment_rate_limited();
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all increments were counted
    assert_eq!(metrics.get_pods_evaluated(), 10);
    assert_eq!(metrics.get_policy_matches(), 10);
    assert_eq!(metrics.get_pods_deleted(), 10);
    assert_eq!(metrics.get_evaluation_errors(), 10);
    assert_eq!(metrics.get_rate_limited(), 10);
}
