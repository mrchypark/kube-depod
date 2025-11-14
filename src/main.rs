use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher::{watcher, Config};
use kube::{Api, Client};
use kube_depod::controller::{load_policies, reconcile_pod_with_rate_limit};
use kube_depod::metrics::Metrics;
use kube_depod::rate_limiter::RateLimiter;
use kube_depod::server::start_server;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting kube-depod operator");

    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // Create metrics instance
    let metrics = Arc::new(Metrics::new());
    let metrics_clone = metrics.clone();

    // Create rate limiter (max 20 deletes per minute)
    let rate_limiter = Arc::new(RateLimiter::new(20));

    // Start metrics HTTP server in background task
    tokio::spawn(async move {
        if let Err(e) = start_server(metrics_clone, 8080).await {
            tracing::error!("Metrics server error: {}", e);
        }
    });

    // Watch for pod changes
    let api: Api<Pod> = Api::all(client.clone());
    let mut stream = watcher(api, Config::default()).boxed();

    // Spawn a background task to periodically reload policies
    let policies_client = client.clone();
    let policies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let policies_clone = policies.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            match load_policies(&policies_client).await {
                Ok(p) => {
                    let mut policies_mut = policies_clone.lock().await;
                    *policies_mut = p.clone();
                    if p.is_empty() {
                        tracing::debug!("No policies loaded");
                    } else {
                        info!(count = p.len(), "Policies reloaded");
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to reload policies: {}", e);
                }
            }
        }
    });

    // Spawn a background task to periodically re-evaluate all pods
    let reevaluate_client = client.clone();
    let reevaluate_policies = policies.clone();
    let reevaluate_metrics = metrics.clone();
    let reevaluate_rate_limiter = rate_limiter.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            let api: Api<Pod> = Api::all(reevaluate_client.clone());
            match api.list(&Default::default()).await {
                Ok(pods) => {
                    info!(count = pods.items.len(), "Re-evaluating all pods");
                    for pod in pods.items {
                        let current_policies = reevaluate_policies.lock().await;
                        reevaluate_metrics.increment_pods_evaluated();
                        match reconcile_pod_with_rate_limit(
                            pod,
                            &current_policies,
                            &reevaluate_client,
                            reevaluate_rate_limiter.clone(),
                        )
                        .await
                        {
                            Ok(result) => {
                                if result.deleted {
                                    reevaluate_metrics.increment_pods_deleted();
                                }
                                if result.matches > 0 {
                                    reevaluate_metrics.increment_policy_matches();
                                }
                                if result.errors > 0 {
                                    reevaluate_metrics.increment_evaluation_errors();
                                }
                                if result.rate_limited {
                                    reevaluate_metrics.increment_rate_limited();
                                }
                            }
                            Err(e) => {
                                reevaluate_metrics.increment_evaluation_errors();
                                tracing::warn!("Re-evaluation reconciliation failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to list pods for re-evaluation: {}", e);
                }
            }
        }
    });

    // Load policies initially
    {
        let initial_policies = match load_policies(&client).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to load initial policies: {}", e);
                Vec::new()
            }
        };
        if !initial_policies.is_empty() {
            info!(count = initial_policies.len(), "Initial policies loaded");
        }
        let mut policies_mut = policies.lock().await;
        *policies_mut = initial_policies;
    }

    // Process pod events
    loop {
        match futures::stream::StreamExt::next(&mut stream).await {
            Some(event) => {
                match event {
                    Ok(watch_event) => {
                        use kube::runtime::watcher::Event;

                        match watch_event {
                            Event::Applied(pod) => {
                                metrics.increment_pods_evaluated();
                                let current_policies = policies.lock().await;
                                match reconcile_pod_with_rate_limit(pod, &current_policies, &client, rate_limiter.clone()).await {
                                    Ok(result) => {
                                        if result.deleted {
                                            metrics.increment_pods_deleted();
                                        }
                                        if result.matches > 0 {
                                            metrics.increment_policy_matches();
                                        }
                                        if result.errors > 0 {
                                            metrics.increment_evaluation_errors();
                                        }
                                        if result.rate_limited {
                                            metrics.increment_rate_limited();
                                        }
                                    }
                                    Err(e) => {
                                        metrics.increment_evaluation_errors();
                                        tracing::warn!("Reconciliation failed: {}", e);
                                    }
                                }
                            }
                            Event::Deleted(_pod) => {
                                // Pod is already deleted, nothing to do
                            }
                            Event::Restarted(pods) => {
                                info!(count = pods.len(), "Pod watcher restarted");
                            }
                        }
                    }
                    Err(e) => {
                        metrics.increment_evaluation_errors();
                        tracing::error!("Watch error: {}", e);
                        // Continue watching
                    }
                }
            }
            None => {
                info!("Pod watcher ended, restarting...");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}
