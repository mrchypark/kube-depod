use crate::crd::DepodPolicy;
use crate::engine::CelEvaluator;
use crate::rate_limiter::RateLimiter;
use crate::{Context, Result};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::policy::v1::Eviction;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{DeleteOptions, ObjectMeta};
use kube::api::{Api, DeleteParams, ListParams, PostParams};
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
pub fn matches_policy(pod: &Pod, policy: &DepodPolicy) -> bool {
    let spec = &policy.spec;

    // Check namespace
    if let Some(ns_selector) = &spec.match_.namespace_selector {
        let pod_ns = pod.namespace().unwrap_or_default();
        if !ns_selector.match_names.is_empty() && !ns_selector.match_names.contains(&pod_ns) {
            return false;
        }
    }

    // Check pod labels
    if let Some(pod_selector) = &spec.match_.pod_selector {
        if !pod_selector.match_labels.is_empty() {
            let pod_labels = pod.labels();
            for (key, value) in &pod_selector.match_labels {
                if pod_labels.get(key.as_str()) != Some(&value.to_string()) {
                    return false;
                }
            }
        }
    }

    true
}

/// Checks if pod has the required annotation
pub fn check_trigger(pod: &Pod, annotation_key: &str, annotation_values: &[String]) -> bool {
    if let Some(annotations) = &pod.metadata.annotations {
        if let Some(value) = annotations.get(annotation_key) {
            return annotation_values.contains(value);
        }
    }
    false
}

/// Evaluates TTL condition (Builtin type)
pub fn evaluate_ttl_condition(pod: &Pod, ttl_seconds: i64) -> bool {
    if let Some(creation_timestamp) = &pod.metadata.creation_timestamp {
        let created = creation_timestamp.0;
        let now = chrono::Utc::now();

        let age = (now - created).num_seconds();
        age > ttl_seconds
    } else {
        false
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

/// Main reconciliation logic for a pod
pub async fn reconcile_pod_with_evaluator(
    pod: Pod,
    policies: &[DepodPolicy],
    client: &Client,
    evaluator: Arc<CelEvaluator>,
    rate_limiter: Option<Arc<RateLimiter>>,
) -> Result<ReconcileResult> {
    let pod_name = pod.name_any();
    let pod_ns = pod.namespace().unwrap_or_default();

    debug!("Reconciling pod {}/{}", pod_ns, pod_name);

    let mut result = ReconcileResult::default();

    for policy in policies {
        if policy.spec.validate().is_err() {
            warn!(
                "Invalid policy {}: {:?}",
                policy.name_any(),
                policy.spec.validate()
            );
            result.errors += 1;
            continue;
        }

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

        result.matches += 1;

        // Evaluate when condition
        let condition_met = match policy.spec.when.condition_type.as_str() {
            "Builtin" => {
                if let Some(ttl_seconds) = policy.spec.when.ttl_seconds {
                    evaluate_ttl_condition(&pod, ttl_seconds)
                } else {
                    false
                }
            }
            "CEL" => {
                if let Some(expr) = &policy.spec.when.expression {
                    match evaluator.evaluate(expr, &pod) {
                        Ok(condition_result) => {
                            debug!(
                                "CEL evaluation for pod {}/{}: {} = {}",
                                pod_ns, pod_name, expr, condition_result
                            );
                            condition_result
                        }
                        Err(crate::Error::CelCompilationError(e)) => {
                            warn!(
                                "CEL compilation failed for policy {}: {}",
                                policy.name_any(), e
                            );
                            result.errors += 1;
                            false
                        }
                        Err(crate::Error::CelEvaluationError(e)) => {
                            warn!(
                                "CEL evaluation failed for pod {}/{} (policy {}): {}",
                                pod_ns, pod_name, policy.name_any(), e
                            );
                            result.errors += 1;
                            false
                        }
                        Err(e) => {
                            warn!(
                                "CEL error for pod {}/{} (policy {}): {}",
                                pod_ns, pod_name, policy.name_any(), e
                            );
                            result.errors += 1;
                            false
                        }
                    }
                } else {
                    warn!(
                        "CEL condition missing expression for policy {}",
                        policy.name_any()
                    );
                    result.errors += 1;
                    false
                }
            }
            _ => {
                warn!(
                    "Unknown condition type: {}",
                    policy.spec.when.condition_type
                );
                result.errors += 1;
                false
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
            let protection_reason = if policy.spec.limits.protect_system_namespaces
                && is_system_namespace(&pod_ns)
            {
                "system namespace"
            } else {
                "excluded namespace"
            };

            info!(
                "Skipping deletion of pod {}/{} ({}), policy: {}",
                pod_ns, pod_name, protection_reason, policy.name_any()
            );
            continue;
        }

        // Execute then action
        match policy.spec.then.action_type.as_str() {
            "Delete" => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would delete pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check rate limit if enabled
                    if let Some(limiter) = &rate_limiter {
                        if !limiter.allow() {
                            info!(
                                "Rate limit exceeded for pod {}/{} (policy: {})",
                                pod_ns,
                                pod_name,
                                policy.name_any()
                            );
                            result.rate_limited = true;
                            continue;
                        }
                    }

                    info!(
                        "Deleting pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );

                    let api: Api<Pod> = Api::namespaced(client.clone(), &pod_ns);
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
                            result.deleted = true;
                        }
                        Err(e) => {
                            warn!("Failed to delete pod {}/{}: {}", pod_ns, pod_name, e);
                            result.errors += 1;
                        }
                    }
                }
            }
            "Evict" => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would evict pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check rate limit if enabled
                    if let Some(limiter) = &rate_limiter {
                        if !limiter.allow() {
                            info!(
                                "Rate limit exceeded for pod {}/{} (policy: {})",
                                pod_ns,
                                pod_name,
                                policy.name_any()
                            );
                            result.rate_limited = true;
                            continue;
                        }
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

                    // Create the eviction request using Policy API v1
                    // This respects Pod Disruption Budgets (PDBs) unlike direct deletion
                    let api: Api<Eviction> = Api::all(client.clone());

                    match api.create(&PostParams::default(), &eviction).await {
                        Ok(_) => {
                            info!(
                                "Successfully evicted pod {}/{} (respects PDB)",
                                pod_ns, pod_name
                            );
                            result.deleted = true;
                        }
                        Err(e) => {
                            // If eviction fails (e.g., due to PDB), log as warning
                            warn!(
                                "Failed to evict pod {}/{} (respects PDB): {}",
                                pod_ns, pod_name, e
                            );
                            result.errors += 1;
                        }
                    }
                }
            }
            _ => {
                warn!("Unknown action type: {}", policy.spec.then.action_type);
                result.errors += 1;
            }
        }

        if result.deleted {
            break; // Stop processing further policies after successful deletion
        }
    }

    Ok(result)
}

/// Wrapper for backward compatibility (without CEL evaluator)
pub async fn reconcile_pod(
    pod: Pod,
    policies: &[DepodPolicy],
    client: &Client,
) -> Result<ReconcileResult> {
    let evaluator = Arc::new(CelEvaluator::new());
    reconcile_pod_with_evaluator(pod, policies, client, evaluator, None).await
}

/// Wrapper with rate limiter support
pub async fn reconcile_pod_with_rate_limit(
    pod: Pod,
    policies: &[DepodPolicy],
    client: &Client,
    rate_limiter: Arc<RateLimiter>,
) -> Result<ReconcileResult> {
    let evaluator = Arc::new(CelEvaluator::new());
    reconcile_pod_with_evaluator(pod, policies, client, evaluator, Some(rate_limiter)).await
}

/// Load all policy rules
pub async fn load_policies(client: &Client) -> Result<Vec<DepodPolicy>> {
    let api: Api<DepodPolicy> = Api::all(client.clone());
    let lp = ListParams::default();

    let policies = api.list(&lp).await?;
    info!(count = policies.items.len(), "Loaded policies");

    Ok(policies.items)
}

/// New reconcile function for kube-rs Controller framework
pub async fn reconcile(pod: Arc<Pod>, ctx: Arc<Context>) -> Result<Action> {
    let pod_name = pod.name_any();
    let pod_ns = pod.namespace().unwrap_or_default();

    debug!("Reconciling pod {}/{}", pod_ns, pod_name);

    ctx.metrics.increment_pods_evaluated();

    let policies = ctx.policies.read().await;
    let mut ttl_requeue_seconds: Option<u64> = None;

    for policy in policies.iter() {
        if policy.spec.validate().is_err() {
            warn!(
                "Invalid policy {}: {:?}",
                policy.name_any(),
                policy.spec.validate()
            );
            ctx.metrics.increment_evaluation_errors();
            continue;
        }

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
        let condition_met = match policy.spec.when.condition_type.as_str() {
            "Builtin" => {
                if let Some(ttl_seconds) = policy.spec.when.ttl_seconds {
                    if evaluate_ttl_condition(&pod, ttl_seconds) {
                        debug!("Pod {}/{} meets TTL condition", pod_ns, pod_name);
                        true
                    } else {
                        // Calculate time until TTL expires
                        if let Some(creation_timestamp) = &pod.metadata.creation_timestamp {
                            let created = creation_timestamp.0;
                            let now = chrono::Utc::now();
                            let age = (now - created).num_seconds();
                            let ttl_left = (ttl_seconds - age).max(1) as u64;

                            info!(
                                "Pod {}/{} does not meet TTL ({}/{}s), requeueing in {}s",
                                pod_ns, pod_name, age, ttl_seconds, ttl_left
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
            "CEL" => {
                if let Some(expr) = &policy.spec.when.expression {
                    match ctx.evaluator.evaluate(expr, &pod) {
                        Ok(condition_result) => {
                            debug!(
                                "CEL evaluation for pod {}/{}: {} = {}",
                                pod_ns, pod_name, expr, condition_result
                            );
                            condition_result
                        }
                        Err(crate::Error::CelCompilationError(e)) => {
                            warn!(
                                "CEL compilation failed for policy {}: {}",
                                policy.name_any(), e
                            );
                            ctx.metrics.increment_evaluation_errors();
                            false
                        }
                        Err(crate::Error::CelEvaluationError(e)) => {
                            warn!(
                                "CEL evaluation failed for pod {}/{} (policy {}): {}",
                                pod_ns, pod_name, policy.name_any(), e
                            );
                            ctx.metrics.increment_evaluation_errors();
                            false
                        }
                        Err(e) => {
                            warn!(
                                "CEL error for pod {}/{} (policy {}): {}",
                                pod_ns, pod_name, policy.name_any(), e
                            );
                            ctx.metrics.increment_evaluation_errors();
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
            _ => {
                warn!(
                    "Unknown condition type: {}",
                    policy.spec.when.condition_type
                );
                ctx.metrics.increment_evaluation_errors();
                false
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
            let protection_reason = if policy.spec.limits.protect_system_namespaces
                && is_system_namespace(&pod_ns)
            {
                "system namespace"
            } else {
                "excluded namespace"
            };

            info!(
                "Skipping deletion of pod {}/{} ({}), policy: {}",
                pod_ns, pod_name, protection_reason, policy.name_any()
            );
            continue;
        }

        // Execute then action
        match policy.spec.then.action_type.as_str() {
            "Delete" => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would delete pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check rate limit if enabled
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
            "Evict" => {
                if policy.spec.then.dry_run {
                    info!(
                        "DRY RUN: Would evict pod {}/{} (policy: {})",
                        pod_ns,
                        pod_name,
                        policy.name_any()
                    );
                } else {
                    // Check rate limit if enabled
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

                    // Create the eviction request using Policy API v1
                    // This respects Pod Disruption Budgets (PDBs) unlike direct deletion
                    let api: Api<Eviction> = Api::all(ctx.client.clone());

                    match api.create(&PostParams::default(), &eviction).await {
                        Ok(_) => {
                            info!(
                                "Successfully evicted pod {}/{} (respects PDB)",
                                pod_ns, pod_name
                            );
                            ctx.metrics.increment_pods_deleted();
                            return Ok(Action::await_change());
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
            _ => {
                warn!("Unknown action type: {}", policy.spec.then.action_type);
                ctx.metrics.increment_evaluation_errors();
            }
        }
    }

    // Determine what action to return
    if let Some(ttl_seconds) = ttl_requeue_seconds {
        debug!(
            "Pod {}/{} will be re-evaluated in {}s when TTL expires",
            pod_ns, pod_name, ttl_seconds
        );
        Ok(Action::requeue(Duration::from_secs(ttl_seconds)))
    } else {
        debug!("Pod {}/{} reconciled, waiting for changes", pod_ns, pod_name);
        Ok(Action::await_change())
    }
}

/// Error policy for reconciliation failures
pub fn error_policy(pod: Arc<Pod>, error: &crate::Error, _ctx: Arc<Context>) -> Action {
    warn!("Reconciliation error for pod {}: {}", pod.name_any(), error);
    // Note: cannot await metrics increment here since this is a sync function
    Action::requeue(Duration::from_secs(60))
}
