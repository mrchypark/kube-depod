use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};
use kube_depod::controller::{error_policy, load_policies, reconcile};
use kube_depod::engine::CelEvaluator;
use kube_depod::metrics::Metrics;
use kube_depod::rate_limiter::RateLimiter;
use kube_depod::server::start_server;
use kube_depod::Context;
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

    // Shared state for policies
    let policies = Arc::new(tokio::sync::RwLock::new(Vec::new()));
    let policies_clone = policies.clone();

    // Spawn a background task to periodically reload policies
    let policies_client = client.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            match load_policies(&policies_client).await {
                Ok(p) => {
                    let mut policies_mut = policies_clone.write().await;
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
        let mut policies_mut = policies.write().await;
        *policies_mut = initial_policies;
    }

    // Create Context for shared state
    let ctx = Arc::new(Context {
        client: client.clone(),
        metrics: metrics.clone(),
        evaluator: Arc::new(CelEvaluator::new()),
        policies,
        rate_limiter,
    });

    // Pod API for watching
    let api: Api<Pod> = Api::all(client.clone());

    // Start Kubernetes controller
    info!("Starting Kubernetes controller");
    Controller::new(api, Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok((pod_ref, _action)) => info!("Reconciled {:?}", pod_ref.name),
                Err(e) => tracing::warn!("Reconcile error: {}", e),
            }
        })
        .await;

    info!("Controller finished");
    Ok(())
}
