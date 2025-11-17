use anyhow::Result;
use arc_swap::ArcSwap;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};
use kube_depod::config::Config as OpConfig;
use kube_depod::controller::{
    error_policy_pod, error_policy_policy, load_policies, reconcile_pod, reconcile_policy,
};
use kube_depod::crd::DepodPolicy;
use kube_depod::engine::CelEvaluator;
use kube_depod::metrics::Metrics;
use kube_depod::rate_limiter::RateLimiter;
use kube_depod::server::start_server;
use kube_depod::Context;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting kube-depod operator");

    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // Load configuration from environment variables
    let config = OpConfig::from_env();

    // Create metrics instance
    let metrics = Arc::new(Metrics::new());
    let metrics_clone = metrics.clone();

    // Create rate limiter from config
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_per_minute));

    // Create shutdown broadcast channel
    let (shutdown_tx, _) = broadcast::channel(1);

    // Start metrics HTTP server in background task
    let shutdown_tx_metrics = shutdown_tx.clone();
    let server_port = config.server_port;
    tokio::spawn(async move {
        if let Err(e) = start_server(metrics_clone, server_port, shutdown_tx_metrics.subscribe()).await {
            tracing::error!("Metrics server error: {}", e);
        }
    });

    // Load policies initially (lock-free with ArcSwap)
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

    // Shared state for policies using ArcSwap (lock-free)
    let policies = Arc::new(ArcSwap::new(Arc::new(initial_policies)));

    // Create Context for shared state from config
    let ctx = Arc::new(Context {
        client: client.clone(),
        metrics: metrics.clone(),
        evaluator: Arc::new(CelEvaluator::new()),
        policies,
        rate_limiter,
        operator_pod_name: Arc::new(config.operator_pod_name.clone()),
        periodic_resync_interval: config.periodic_resync_interval,
        pod_patch_concurrency_limit: config.pod_patch_concurrency_limit,
    });

    // Graceful shutdown handler
    let shutdown_tx_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received shutdown signal (SIGINT/SIGTERM)");
        let _ = shutdown_tx_signal.send(());
    });

    // --- Start DepodPolicy controller ---
    let policy_api: Api<DepodPolicy> = Api::all(client.clone());
    let policy_ctx = ctx.clone();

    let policy_handle = tokio::spawn(async move {
        info!("Starting DepodPolicy controller");
        Controller::new(policy_api, Config::default())
            .run(reconcile_policy, error_policy_policy, policy_ctx)
            .for_each(|res| async move {
                match res {
                    Ok((policy_ref, _action)) => info!("Reconciled policy {:?}", policy_ref.name),
                    Err(e) => tracing::warn!("Policy reconcile error: {}", e),
                }
            })
            .await;
    });

    // --- Start Pod controller ---
    let api: Api<Pod> = Api::all(client.clone());

    let pod_handle = tokio::spawn(async move {
        info!("Starting Kubernetes controller");
        Controller::new(api, Config::default())
            .run(reconcile_pod, error_policy_pod, ctx)
            .for_each(|res| async move {
                match res {
                    Ok((pod_ref, _action)) => info!("Reconciled {:?}", pod_ref.name),
                    Err(e) => tracing::warn!("Reconcile error: {}", e),
                }
            })
            .await;
    });

    // Wait for shutdown signal
    let mut shutdown_rx = shutdown_tx.subscribe();
    let _ = shutdown_rx.recv().await;
    info!("Initiating graceful shutdown");

    // Cancel controller tasks
    policy_handle.abort();
    pod_handle.abort();

    info!("Graceful shutdown complete");
    Ok(())
}
