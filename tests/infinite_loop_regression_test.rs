use anyhow::Result;
use arc_swap::ArcSwap;
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use kube::{
    client::Body,
    Client,
};
use kube_depod::controller::reconcile_policy;
use kube_depod::crd::{
    ConditionType, DepodPolicy, DepodPolicySpec, Limits, Match, Then, Trigger, When, ActionType, DepodPolicyStatus, PolicyCondition
};
use kube_depod::engine::CelEvaluator;
use kube_depod::metrics::Metrics;
use kube_depod::rate_limiter::RateLimiter;
use kube_depod::Context;
use std::collections::BTreeSet;
use std::sync::Arc;
use tower_test::mock;

#[tokio::test]
async fn test_infinite_loop_regression_mock() -> Result<()> {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // 1. Setup Mock Client
    let (mock_service, mut handle) = mock::pair::<Request<Body>, Response<Body>>();
    let client = Client::new(mock_service, "default");

    // 2. Setup Context
    let metrics = Arc::new(Metrics::new());
    let rate_limiter = Arc::new(RateLimiter::new(20));
    let (policy_store, _) = kube::runtime::reflector::store();
    let policies = Arc::new(ArcSwap::new(Arc::new(Vec::new())));
    
    let ctx = Arc::new(Context {
        client: client.clone(),
        metrics: metrics.clone(),
        evaluator: Arc::new(CelEvaluator::new()),
        policies,
        policy_store,
        rate_limiter,
        operator_pod_name: Arc::new("test-operator".to_string()),
        periodic_resync_interval: None,
        pod_patch_concurrency_limit: 5,
    });

    // 3. Create Invalid Policy
    let policy_name = "invalid-cel-policy";
    let policy_ns = "default";
    
    let mut policy = DepodPolicy {
        metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
            name: Some(policy_name.to_string()),
            namespace: Some(policy_ns.to_string()),
            ..Default::default()
        },
        spec: DepodPolicySpec {
            match_: Match {
                namespace_selector: None,
                pod_selector: None,
            },
            trigger: Trigger {
                annotation_key: "test/trigger".to_string(),
                annotation_values: BTreeSet::from(["true".to_string()]),
            },
            when: When {
                condition_type: ConditionType::CEL,
                expression: Some("this is invalid syntax !!!".to_string()),
                ttl_seconds: None,
            },
            then: Then {
                action_type: ActionType::Delete,
                grace_period_seconds: Some(0),
                dry_run: false,
            },
            limits: Limits {
                max_deletes_per_minute: None,
                protect_system_namespaces: true,
                excluded_namespaces: None,
            },
        },
        status: None,
    };

    // 4. Run reconcile_policy (First Run)
    // Expect a PATCH request to update status to InvalidCEL
    
    let policy_arc = Arc::new(policy.clone());
    let ctx_clone = ctx.clone();
    
    let reconcile_future = tokio::spawn(async move {
        reconcile_policy(policy_arc, ctx_clone).await
    });

    // Handle expected request
    let (request, send) = handle.next_request().await.expect("Expected PATCH request");
    
    assert_eq!(request.method(), http::Method::PATCH);
    assert_eq!(request.uri().path(), format!("/apis/kube-depod.io/v1alpha1/namespaces/{policy_ns}/depodpolicies/{policy_name}/status"));
    
    // Verify body contains InvalidCEL
    let body_bytes = request.into_body().collect().await?.to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec())?;
    assert!(body_str.contains("InvalidCEL"));
    assert!(body_str.contains("CEL compilation failed"));

    // Send success response
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(serde_json::to_vec(&policy)?))?;
    send.send_response(response);

    let _result = reconcile_future.await??;
    // Expect await_change because validation failed
    // Actually reconcile_policy returns Action::await_change() on error
    // Wait, verify return value
    // It returns Ok(Action::await_change())
    
    // 5. Run reconcile_policy (Second Run)
    // Simulate that the policy now has the status
    // We manually construct the status that matches what we expect
    
    let expr = "this is invalid syntax !!!";
    let err_msg = match cel::Program::compile(expr) {
        Ok(_) => panic!("Expected compilation error"),
        Err(e) => format!("CEL compilation failed: CEL compilation error: {e}"),
    };
    
    let condition = PolicyCondition {
        condition_type: "InvalidCEL".to_string(),
        status: "True".to_string(),
        last_transition_time: Some("2024-01-01T00:00:00Z".to_string()), // Time doesn't matter for equality check logic? 
        // Wait, the logic checks message.
        reason: None,
        message: Some(err_msg.to_string()),
    };
    
    policy.status = Some(DepodPolicyStatus {
        conditions: vec![condition],
        pods_evaluated: 0,
        pods_matched: 0,
        pods_deleted: 0,
        evaluation_errors: 0,
        last_observed_time: None,
    });

    let policy_arc_2 = Arc::new(policy.clone());
    let ctx_clone_2 = ctx.clone();

    let reconcile_future_2 = tokio::spawn(async move {
        reconcile_policy(policy_arc_2, ctx_clone_2).await
    });

    // Expect NO request
    // We wait a bit to ensure no request is sent
    if let Ok(Some(_)) = tokio::time::timeout(std::time::Duration::from_secs(1), handle.next_request()).await {
        panic!("Did not expect any request on second run!");
    }

    let _result_2 = reconcile_future_2.await??;
    
    Ok(())
}
