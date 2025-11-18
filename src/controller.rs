use crate::crd::{DepodPolicy, PolicyCondition};
use crate::status_updater::StatusUpdater;
use crate::{Context, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::policy::v1::Eviction;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{DeleteOptions, ObjectMeta, Status};
use kube::api::{Api, DeleteParams, ListParams, PostParams, Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::{Client, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Result of pod reconciliation
#[derive(Debug, Default)]
pub struct ReconcileResult {
    pub deleted: bool,
    pub matches: u32,
    pub errors: u32,
    pub rate_limited: bool,
}

/// Matches a pod against a policy rule
/// 
/// Performs early exit on namespace/label mismatch for efficiency.
/// Uses direct BTreeSet lookup for namespace matching (O(log n)).
pub fn matches_policy(pod: &Pod, policy: &DepodPolicy) -> bool {
    let spec = &policy.spec;

    // Check namespace - early exit if namespace mismatch
    if let Some(ns_selector) = &spec.match_.namespace_selector {
        if !ns_selector.match_names.is_empty() {
            let pod_ns = pod.namespace().unwrap_or_default();
            if !ns_selector.match_names.contains(&pod_ns) {
                return false;
            }
        }
    }

    // Check pod labels - early exit if any label mismatch
    if let Some(pod_selector) = &spec.match_.pod_selector {
        if !pod_selector.match_labels.is_empty() {
            let pod_labels = pod.labels();
            for (key, value) in &pod_selector.match_labels {
                if pod_labels.get(key.as_str()) != Some(value) {
                    return false;
                }
            }
        }
    }

    true
}

/// Checks if pod has the required annotation
/// 
/// The annotation_values BTreeSet is used directly without conversion,
/// achieving O(log n) lookup performance for large value sets.
pub fn check_trigger(
    pod: &Pod,
    annotation_key: &str,
    annotation_values: &std::collections::BTreeSet<String>,
) -> bool {
    if let Some(annotations) = &pod.metadata.annotations {
        if let Some(value) = annotations.get(annotation_key) {
            return annotation_values.contains(value);
        }
    }
    false
}

/// Evaluates TTL condition (Builtin type)
pub fn evaluate_ttl_condition(pod: &Pod, ttl_seconds: i64, now: chrono::DateTime<chrono::Utc>) -> bool {
    if let Some(creation_timestamp) = &pod.metadata.creation_timestamp {
        let created = creation_timestamp.0;
        let age = (now - created).num_seconds();
        age > ttl_seconds
    } else {
        false
    }
}

/// Calculate seconds until TTL expires
pub fn calculate_ttl_requeue(pod: &Pod, ttl_seconds: i64, now: chrono::DateTime<chrono::Utc>) -> Option<u64> {
    if let Some(creation_timestamp) = &pod.metadata.creation_timestamp {
        let created = creation_timestamp.0;
        let age = (now - created).num_seconds();
        Some((ttl_seconds - age).max(1) as u64)
    } else {
        None
    }
}

/// Check if namespace should be protected
pub fn is_system_namespace(ns: &str) -> bool {
    matches!(
        ns,
        "kube-system"
            | "kube-public"
            | "kube-node-lease"
            | "kube-apiserver"
            | "kube-controller-manager"
            | "kube-scheduler"
    )
}

/// Check if namespace is protected by policy limits
pub fn is_namespace_protected(
    ns: &str,
    protect_system: bool,
    excluded_namespaces: &Option<Vec<String>>,
) -> bool {
    // Check if it's a system namespace
    if protect_system && is_system_namespace(ns) {
        return true;
    }

    // Check if it's in the excluded namespaces list
    if let Some(excluded) = excluded_namespaces {
        if excluded.contains(&ns.to_string()) {
            return true;
        }
    }

    false
}

/// Update policy status with a condition
/// 
/// This function uses StatusUpdater to safely update the policy status.
async fn update_policy_status(
    client: &Client,
    policy_name: &str,
    namespace: &str,
    condition: PolicyCondition,
) -> Result<()> {
    let api: Api<DepodPolicy> = Api::namespaced(client.clone(), namespace);
    
    // Use StatusUpdater to ensure consistent status updates
    let mut status = crate::crd::DepodPolicyStatus::default();
    StatusUpdater::update_condition(&mut status, condition);
    
    let patch_params = PatchParams::default();
    let patch = Patch::Merge(serde_json::json!({
        "status": status
    }));

    api.patch_subresource("status", policy_name, &patch_params, &patch)
        .await?;

    Ok(())
}

/// Load all policy rules with validation
///
/// This function loads policies and filters out invalid ones at load time.
/// Invalid policies are logged as warnings but excluded from the result.
/// This ensures that only valid policies are cached and used in the hot path.
/// New reconcile function for kube-rs Controller framework (Pod-specific)
pub async fn reconcile_pod(pod: Arc<Pod>, ctx: Arc<Context>) -> Result<Action> {
    let pod_name = pod.name_any();
    let pod_ns = pod.namespace().unwrap_or_default();

    debug!("Reconciling pod {}/{}", pod_ns, pod_name);

    ctx.metrics.increment_pods_evaluated();

    // Load policies (lock-free with ArcSwap - no await, no blocking)
    let policies = ctx.policies.load();
    let mut ttl_requeue_seconds: Option<u64> = None;

    for policy in policies.iter() {
        if !matches_policy(&pod, policy) {
            debug!(
                "Pod {}/{} does not match policy {}",
                pod_ns,
                pod_name,
                policy.name_any()
            );
            continue;
        }

        if !check_trigger(
            &pod,
            &policy.spec.trigger.annotation_key,
            &policy.spec.trigger.annotation_values,
        ) {
            debug!(
                "Pod {}/{} does not have trigger annotation for policy {}",
                pod_ns,
                pod_name,
                policy.name_any()
            );
            continue;
        }

        ctx.metrics.increment_policy_matches();

        // Evaluate when condition
        let condition_met = match &policy.spec.when.condition_type {
            crate::crd::ConditionType::Builtin => {
                if let Some(ttl_seconds) = policy.spec.when.ttl_seconds {
                    let now = chrono::Utc::now();
                    if evaluate_ttl_condition(&pod, ttl_seconds, now) {
                        debug!("Pod {}/{} meets TTL condition", pod_ns, pod_name);
                        true
                    } else {
                        // Calculate time until TTL expires
                        if let Some(ttl_left) = calculate_ttl_requeue(&pod, ttl_seconds, now) {
                            info!(
                                "Pod {}/{} does not meet TTL ({}/{}s), requeueing in {}s",
                                pod_ns, pod_name, (now - pod.metadata.creation_timestamp.as_ref().unwrap().0).num_seconds(), ttl_seconds, ttl_left
                            );

                            // Store the requeue time (use the minimum if multiple policies)
                            if let Some(current) = ttl_requeue_seconds {
                                ttl_requeue_seconds = Some(current.min(ttl_left));
                            } else {
                                ttl_requeue_seconds = Some(ttl_left);
                            }
                        }
                        false
                    }
                } else {
                    warn!("Builtin policy {} missing ttlSeconds", policy.name_any());
                    ctx.metrics.increment_evaluation_errors();
                    false
                }
            }
            crate::crd::ConditionType::CEL => {
                if let Some(expr) = &policy.spec.when.expression {
                    match ctx.evaluator.evaluate(expr, &pod, &policy.name_any()) {
                        Ok(condition_result) => {
                            debug!(
                                "CEL evaluation for pod {}/{}: {} = {}",
                                pod_ns, pod_name, expr, condition_result
                            );
                            condition_result
                        }
                        Err(crate::Error::CelCompilationError(e)) => {
                            let err_msg = format!("CEL compilation failed: {}", e);
                            warn!(
                                "CEL compilation failed for policy {}: {}",
                                policy.name_any(),
                                e
                            );
                            ctx.metrics.increment_evaluation_errors();
                            
                            // Update policy status with InvalidCEL condition
                            let policy_ns = policy.namespace().unwrap_or_else(|| "default".to_string());
                            let condition = PolicyCondition::invalid_cel(&err_msg);
                            if let Err(status_err) = update_policy_status(
                                &ctx.client,
                                &policy.name_any(),
                                &policy_ns,
                                condition,
                            )
                            .await
                            {
                                warn!("Failed to update policy status for {}: {}", policy.name_any(), status_err);
                            }
                            false
                        }
                        Err(crate::Error::CelEvaluationError(e)) => {
                            let err_msg = format!("CEL evaluation failed: {}", e);
                            warn!(
                                "CEL evaluation failed for pod {}/{} (policy {}): {}",
                                pod_ns,
                                pod_name,
                                policy.name_any(),
                                e
                            );
                            ctx.metrics.increment_evaluation_errors();
                            
                            // Update policy status with InvalidCEL condition
                            let policy_ns = policy.namespace().unwrap_or_else(|| "default".to_string());
                            let condition = PolicyCondition::invalid_cel(&err_msg);
                            if let Err(status_err) = update_policy_status(
                                &ctx.client,
                                &policy.name_any(),
                                &policy_ns,
                                condition,
                            )
                            .await
                            {
                                warn!("Failed to update policy status for {}: {}", policy.name_any(), status_err);
                            }
                            false
                        }
                        Err(e) => {
                            let err_msg = format!("CEL error: {}", e);
                            warn!(
                                "CEL error for pod {}/{} (policy {}): {}",
                                pod_ns,
                                pod_name,
                                policy.name_any(),
                                e
                            );
                            ctx.metrics.increment_evaluation_errors();
                            
                            // Update policy status with InvalidCEL condition
                            let policy_ns = policy.namespace().unwrap_or_else(|| "default".to_string());
                            let condition = PolicyCondition::invalid_cel(&err_msg);
                            if let Err(status_err) = update_policy_status(
                                &ctx.client,
                                &policy.name_any(),
                                &policy_ns,
                                condition,
                            )
                            .await
                            {
                                warn!("Failed to update policy status for {}: {}", policy.name_any(), status_err);
                            }
                            false
                        }
                    }
                } else {
                    warn!(
                        "CEL condition missing expression for policy {}",
                        policy.name_any()
                    );
                    ctx.metrics.increment_evaluation_errors();
                    false
                }
            }
        };

        if !condition_met {
            debug!(
                "Condition not met for pod {}/{} under policy {}",
                pod_ns,
                pod_name,
                policy.name_any()
            );
            continue;
        }

        // Check safety limits (system namespaces + excluded namespaces)
        if is_namespace_protected(
            &pod_ns,
            policy.spec.limits.protect_system_namespaces,
            &policy.spec.limits.excluded_namespaces,
        ) {
            let protection_reason =
                if policy.spec.limits.protect_system_namespaces && is_system_namespace(&pod_ns) {
                    "system namespace"
                } else {
                    "excluded namespace"
                };

            info!(
                "Skipping deletion of pod {}/{} ({}), policy: {}",
                pod_ns,
                pod_name,
                protection_reason,
                policy.name_any()
            );
            continue;
        }

        // Execute then action
        match &policy.spec.then.action_type {
            crate::crd::ActionType::Delete => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would delete pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check global rate limit (configured via RATE_LIMIT_PER_MINUTE env, default: 20)
                    if !ctx.rate_limiter.allow() {
                        info!(
                            "Rate limit exceeded for pod {}/{} (policy: {})",
                            pod_ns,
                            pod_name,
                            policy.name_any()
                        );
                        ctx.metrics.increment_rate_limited();
                        // Requeue after 10 seconds to avoid hammering the API
                        return Ok(Action::requeue(Duration::from_secs(10)));
                    }

                    info!(
                        "Deleting pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );

                    let api: Api<Pod> = Api::namespaced(ctx.client.clone(), &pod_ns);
                    let dp = DeleteParams {
                        grace_period_seconds: policy
                            .spec
                            .then
                            .grace_period_seconds
                            .map(|g| g as u32),
                        ..Default::default()
                    };

                    match api.delete(&pod_name, &dp).await {
                        Ok(_) => {
                            info!("Successfully deleted pod {}/{}", pod_ns, pod_name);
                            ctx.metrics.increment_pods_deleted();
                            return Ok(Action::await_change());
                        }
                        Err(e) => {
                            warn!("Failed to delete pod {}/{}: {}", pod_ns, pod_name, e);
                            ctx.metrics.increment_evaluation_errors();
                        }
                    }
                }
            }
            crate::crd::ActionType::Evict => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would evict pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check global rate limit (configured via RATE_LIMIT_PER_MINUTE env, default: 20)
                    if !ctx.rate_limiter.allow() {
                        info!(
                            "Rate limit exceeded for pod {}/{} (policy: {})",
                            pod_ns,
                            pod_name,
                            policy.name_any()
                        );
                        ctx.metrics.increment_rate_limited();
                        // Requeue after 10 seconds to avoid hammering the API
                        return Ok(Action::requeue(Duration::from_secs(10)));
                    }

                    info!(
                        "Evicting pod {}/{} (policy: {}) - respects Pod Disruption Budgets",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );

                    // Create eviction request respecting Pod Disruption Budgets
                    let eviction = Eviction {
                        metadata: ObjectMeta {
                            name: Some(pod_name.clone()),
                            namespace: Some(pod_ns.clone()),
                            ..Default::default()
                        },
                        delete_options: Some(DeleteOptions {
                            grace_period_seconds: policy.spec.then.grace_period_seconds,
                            ..Default::default()
                        }),
                    };

                    // Use Pod subresource for eviction to respect Pod Disruption Budgets (PDBs)
                    // This is the correct way to invoke the eviction API in Kubernetes
                    let pods: Api<Pod> = Api::namespaced(ctx.client.clone(), &pod_ns);
                    let eviction_bytes = serde_json::to_vec(&eviction)?;

                    match pods
                        .create_subresource::<Status>("eviction", &pod_name, &PostParams::default(), eviction_bytes)
                        .await
                    {
                        Ok(status) => {
                            if let Some(ref status_str) = status.status {
                                if status_str == "Success" {
                                    info!(
                                        "Successfully evicted pod {}/{} (respects PDB)",
                                        pod_ns, pod_name
                                    );
                                    ctx.metrics.increment_pods_deleted();
                                    return Ok(Action::await_change());
                                } else {
                                    warn!(
                                        "Eviction of pod {}/{} returned non-success status: {}",
                                        pod_ns, pod_name, status_str
                                    );
                                    ctx.metrics.increment_evaluation_errors();
                                }
                            } else {
                                info!(
                                    "Successfully evicted pod {}/{} (respects PDB)",
                                    pod_ns, pod_name
                                );
                                ctx.metrics.increment_pods_deleted();
                                return Ok(Action::await_change());
                            }
                        }
                        Err(e) => {
                            // If eviction fails (e.g., due to PDB), log as warning
                            warn!(
                                "Failed to evict pod {}/{} (respects PDB): {}",
                                pod_ns, pod_name, e
                            );
                            ctx.metrics.increment_evaluation_errors();
                        }
                    }
                }
            }
        }
    }

    // Determine what action to return
    if let Some(ttl_seconds) = ttl_requeue_seconds {
        // TTL expiration is the closest event, requeue based on TTL
        debug!(
            "Pod {}/{} will be re-evaluated in {}s when TTL expires",
            pod_ns, pod_name, ttl_seconds
        );
        Ok(Action::requeue(Duration::from_secs(ttl_seconds)))
    } else if let Some(interval) = ctx.periodic_resync_interval {
        // No TTL pending, but cron check is enabled - schedule periodic resync
        debug!(
            "Pod {}/{} reconciled, scheduling periodic resync in {:?}",
            pod_ns, pod_name, interval
        );
        Ok(Action::requeue(interval))
    } else {
        // No TTL pending and cron check disabled - wait for changes
        debug!(
            "Pod {}/{} reconciled, waiting for changes",
            pod_ns, pod_name
        );
        Ok(Action::await_change())
    }
}

/// Error policy for reconciliation failures (Pod-specific)
pub fn error_policy_pod(pod: Arc<Pod>, error: &crate::Error, _ctx: Arc<Context>) -> Action {
    warn!("Reconciliation error for pod {}: {}", pod.name_any(), error);
    // Note: cannot await metrics increment here since this is a sync function
    Action::requeue(Duration::from_secs(60))
}

/// DepodPolicy controller error handler
pub fn error_policy_policy(
    _policy: Arc<DepodPolicy>,
    error: &crate::Error,
    _ctx: Arc<Context>,
) -> Action {
    warn!("Policy reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}

/// DepodPolicy reconcile function - updates policy cache and triggers pod re-evaluation
pub async fn reconcile_policy(policy: Arc<DepodPolicy>, ctx: Arc<Context>) -> Result<Action> {
    use serde_json::json;

    let policy_name = policy.name_any();
    let policy_ns = policy.namespace().unwrap_or_else(|| "default".to_string());

    // --- Validation: Early exit if policy is invalid ---
    if let Err(e) = policy.spec.validate() {
        warn!(
            "Skipping invalid policy {}: {}. Cache will not include this policy.",
            policy_name, e
        );
        
        // Update policy status with InvalidSpec condition
        let condition = PolicyCondition::invalid_spec(e.to_string());
        if let Err(status_err) = update_policy_status(&ctx.client, &policy_name, &policy_ns, condition).await {
            warn!("Failed to update policy status for {}: {}", policy_name, status_err);
        }
        
        return Ok(Action::await_change());
    }

    info!(
        "Policy {} changed or detected, updating cache and re-triggering pods",
        policy_name
    );

    // --- 1. Update policy cache from Store (lock-free with ArcSwap) ---
    // Read all policies from the Reflector's Store instead of making API calls
    let all_policies: Vec<DepodPolicy> = ctx.policy_store.state()
        .iter()
        .filter_map(|policy_arc| {
            // Validate each policy from store
            if policy_arc.spec.validate().is_ok() {
                Some((**policy_arc).clone())
            } else {
                None
            }
        })
        .collect();

    ctx.policies.store(Arc::new(all_policies.clone()));
    info!(
        count = all_policies.len(),
        "Policy cache refreshed from store due to {} change",
        policy_name
    );
    
    // Update policy status with Ready condition
    let condition = PolicyCondition::ready();
    if let Err(status_err) = update_policy_status(&ctx.client, &policy_name, &policy_ns, condition).await {
        warn!("Failed to update policy status for {}: {}", policy_name, status_err);
    }

    // --- 2. Find and touch all matching pods ---

    // Build label selector from pod_selector
    let mut lp = ListParams::default();
    if let Some(pod_selector) = &policy.spec.match_.pod_selector {
        let labels = pod_selector
            .match_labels
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");
        if !labels.is_empty() {
            lp = lp.labels(&labels);
        }
    }

    // Get namespace selector
    let namespaces: Vec<String> = if let Some(ns_selector) = &policy.spec.match_.namespace_selector
    {
        if ns_selector.match_names.is_empty() {
            // Empty match_names means all namespaces
            vec![]
        } else {
            ns_selector.match_names.clone()
        }
    } else {
        // No namespace selector means all namespaces
        vec![]
    };

    // Patch params for triggering reconciliation
    let patch_params = PatchParams::default();
    let patch = Patch::Merge(json!({
        "metadata": {
            "annotations": {
                "kube-depod/reconcile-trigger-by": ctx.operator_pod_name.as_str(),
                "kube-depod/reconcile-trigger-by-policy": &policy_name,
                "kube-depod/reconcile-trigger-time": &chrono::Utc::now().to_rfc3339()
            }
        }
    }));

    // If namespaces are restricted, iterate per namespace; otherwise use Api::all
    if namespaces.is_empty() {
        // All namespaces: use Api::all for efficiency
        let pod_api: Api<Pod> = Api::all(ctx.client.clone());
        let pods_to_trigger = match pod_api.list(&lp).await {
            Ok(pods) => pods,
            Err(e) => {
                warn!("Failed to list pods for policy {}: {}", policy_name, e);
                return Ok(Action::requeue(Duration::from_secs(60)));
            }
        };

        // Parallel pod touch with concurrency limit
        let policy_for_filter = policy.clone();
        futures::stream::iter(pods_to_trigger)
            .filter_map(move |pod| {
                let policy = policy_for_filter.clone();
                async move {
                    // Opt-in check: does this pod have the trigger annotation?
                    if !check_trigger(
                        &pod,
                        &policy.spec.trigger.annotation_key,
                        &policy.spec.trigger.annotation_values,
                    ) {
                        return None;
                    }

                    // Safety guardrail: does this pod match the policy's full scope?
                    if !matches_policy(&pod, &policy) {
                        return None;
                    }

                    Some(pod)
                }
            })
            .for_each_concurrent(Some(ctx.pod_patch_concurrency_limit), |pod| {
                let pod_api = pod_api.clone();
                let patch_params = patch_params.clone();
                let patch = patch.clone();
                let policy_name = policy_name.clone();

                async move {
                    let pod_name = pod.name_any();
                    let pod_ns = pod.namespace().unwrap_or_default();
                    info!(
                        "Triggering reconciliation for Pod {}/{} (policy: {})",
                        pod_ns, pod_name, policy_name
                    );

                    // Touch pod to trigger Modify event in Pod controller
                    if let Err(e) = pod_api.patch(&pod_name, &patch_params, &patch).await {
                        warn!("Failed to 'touch' pod {}/{}: {}", pod_ns, pod_name, e);
                    }
                }
            })
            .await;
    } else {
        // Restricted namespaces: iterate per namespace for efficiency
        for ns in &namespaces {
            let pod_api: Api<Pod> = Api::namespaced(ctx.client.clone(), ns);
            let pods_to_trigger = match pod_api.list(&lp).await {
                Ok(pods) => pods,
                Err(e) => {
                    warn!(
                        "Failed to list pods in namespace {} for policy {}: {}",
                        ns, policy_name, e
                    );
                    continue;
                }
            };

            // Parallel pod touch with concurrency limit
            let policy_for_filter = policy.clone();
            futures::stream::iter(pods_to_trigger)
                .filter_map(move |pod| {
                    let policy = policy_for_filter.clone();
                    async move {
                        // Opt-in check: does this pod have the trigger annotation?
                        if !check_trigger(
                            &pod,
                            &policy.spec.trigger.annotation_key,
                            &policy.spec.trigger.annotation_values,
                        ) {
                            return None;
                        }

                        // Safety guardrail: does this pod match the policy's full scope?
                        if !matches_policy(&pod, &policy) {
                            return None;
                        }

                        Some(pod)
                    }
                })
                .for_each_concurrent(Some(ctx.pod_patch_concurrency_limit), |pod| {
                    let pod_api = pod_api.clone();
                    let patch_params = patch_params.clone();
                    let patch = patch.clone();
                    let policy_name = policy_name.clone();

                    async move {
                        let pod_name = pod.name_any();
                        let pod_ns = pod.namespace().unwrap_or_default();
                        info!(
                            "Triggering reconciliation for Pod {}/{} (policy: {})",
                            pod_ns, pod_name, policy_name
                        );

                        // Touch pod to trigger Modify event in Pod controller
                        if let Err(e) = pod_api.patch(&pod_name, &patch_params, &patch).await {
                            warn!("Failed to 'touch' pod {}/{}: {}", pod_ns, pod_name, e);
                        }
                    }
                })
                .await;
        }
    }

    // Wait for next policy change event
    Ok(Action::await_change())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::{DepodPolicySpec, Match, NamespaceSelector, PodSelector, Trigger, When, Then, ActionType, ConditionType, Limits};
    use std::collections::{BTreeMap, BTreeSet};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn create_test_policy(
        namespace_selector: Option<NamespaceSelector>,
        pod_selector: Option<PodSelector>,
        annotation_key: &str,
        annotation_values: BTreeSet<String>,
    ) -> DepodPolicy {
        DepodPolicy {
            metadata: ObjectMeta {
                name: Some("test-policy".to_string()),
                ..Default::default()
            },
            spec: DepodPolicySpec {
                match_: Match {
                    namespace_selector,
                    pod_selector,
                },
                trigger: Trigger {
                    annotation_key: annotation_key.to_string(),
                    annotation_values,
                },
                when: When {
                    condition_type: ConditionType::Builtin,
                    expression: None,
                    ttl_seconds: Some(60),
                },
                then: Then {
                    action_type: ActionType::Delete,
                    grace_period_seconds: None,
                    dry_run: false,
                },
                limits: Limits {
                    max_deletes_per_minute: None,
                    protect_system_namespaces: true,
                    excluded_namespaces: None,
                },
            },
            status: None,
        }
    }

    #[test]
    fn test_matches_policy_namespace() {
        let mut policy = create_test_policy(
            Some(NamespaceSelector {
                match_names: vec!["allowed-ns".to_string()],
            }),
            None,
            "test-key",
            BTreeSet::new(),
        );

        let mut pod = Pod::default();
        pod.metadata.namespace = Some("allowed-ns".to_string());
        assert!(matches_policy(&pod, &policy));

        pod.metadata.namespace = Some("other-ns".to_string());
        assert!(!matches_policy(&pod, &policy));

        // Empty match_names means all namespaces (if selector is present but empty list? No, logic says if not empty check. If empty list, it skips check)
        // Let's check logic: if !ns_selector.match_names.is_empty() { check }
        // So empty list = match all
        policy.spec.match_.namespace_selector = Some(NamespaceSelector { match_names: vec![] });
        assert!(matches_policy(&pod, &policy));
    }

    #[test]
    fn test_matches_policy_labels() {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "test".to_string());
        
        let policy = create_test_policy(
            None,
            Some(PodSelector {
                match_labels: labels.clone(),
            }),
            "test-key",
            BTreeSet::new(),
        );

        let mut pod = Pod::default();
        pod.metadata.labels = Some(labels);
        assert!(matches_policy(&pod, &policy));

        let mut wrong_labels = BTreeMap::new();
        wrong_labels.insert("app".to_string(), "other".to_string());
        pod.metadata.labels = Some(wrong_labels);
        assert!(!matches_policy(&pod, &policy));
    }

    #[test]
    fn test_check_trigger() {
        let key = "kube-depod/test";
        let values = BTreeSet::from(["true".to_string(), "yes".to_string()]);

        let mut pod = Pod::default();
        let mut annotations = BTreeMap::new();
        
        // Case 1: Annotation present and matching
        annotations.insert(key.to_string(), "true".to_string());
        pod.metadata.annotations = Some(annotations.clone());
        assert!(check_trigger(&pod, key, &values));

        // Case 2: Annotation present but value mismatch
        annotations.insert(key.to_string(), "no".to_string());
        pod.metadata.annotations = Some(annotations.clone());
        assert!(!check_trigger(&pod, key, &values));

        // Case 3: Annotation missing
        pod.metadata.annotations = None;
        assert!(!check_trigger(&pod, key, &values));
    }
}
