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

    loop {
        // Load policies periodically
        let policies = match load_policies(&client).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to load policies: {}", e);
                Vec::new()
            }
        };

        if policies.is_empty() {
            tracing::debug!("No policies loaded");
        } else {
            info!(count = policies.len(), "Policies loaded");
        }

        // Process pod events
        while let Some(event) = futures::stream::StreamExt::next(&mut stream).await {
            match event {
                Ok(watch_event) => {
                    use kube::runtime::watcher::Event;

                    match watch_event {
                        Event::Applied(pod) => {
                            metrics.increment_pods_evaluated();
                            match reconcile_pod_with_rate_limit(pod, &policies, &client, rate_limiter.clone()).await {
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

        info!("Pod watcher ended, restarting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
