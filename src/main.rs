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
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;
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
    let shutdown_rx_policy = shutdown_tx.subscribe();

    let policy_handle = tokio::spawn(async move {
        info!("Starting DepodPolicy controller");
        let mut shutdown_rx = shutdown_rx_policy;
        
        // Create a cancellation token for the controller
        let controller_future = Box::pin(
            Controller::new(policy_api, Config::default())
                .run(reconcile_policy, error_policy_policy, policy_ctx)
                .for_each(|res| async move {
                    match res {
                        Ok((policy_ref, _action)) => info!("Reconciled policy {:?}", policy_ref.name),
                        Err(e) => tracing::warn!("Policy reconcile error: {}", e),
                    }
                })
        );

        // Run controller with shutdown signal monitoring
        tokio::select! {
            _ = controller_future => {
                info!("DepodPolicy controller exited");
            }
            _ = shutdown_rx.recv() => {
                info!("DepodPolicy controller received shutdown signal");
            }
        }
    });

    // --- Start Pod controller ---
    let api: Api<Pod> = Api::all(client.clone());
    let shutdown_rx_pod = shutdown_tx.subscribe();

    let pod_handle = tokio::spawn(async move {
        info!("Starting Kubernetes controller");
        let mut shutdown_rx = shutdown_rx_pod;
        
        let controller_future = Box::pin(
            Controller::new(api, Config::default())
                .run(reconcile_pod, error_policy_pod, ctx)
                .for_each(|res| async move {
                    match res {
                        Ok((pod_ref, _action)) => info!("Reconciled {:?}", pod_ref.name),
                        Err(e) => tracing::warn!("Reconcile error: {}", e),
                    }
                })
        );

        // Run controller with shutdown signal monitoring
        tokio::select! {
            _ = controller_future => {
                info!("Pod controller exited");
            }
            _ = shutdown_rx.recv() => {
                info!("Pod controller received shutdown signal");
            }
        }
    });

    // Wait for shutdown signal
    let mut shutdown_rx = shutdown_tx.subscribe();
    let _ = shutdown_rx.recv().await;
    info!("Initiating graceful shutdown");

    // Stage 1: Graceful shutdown - wait for controllers to finish
    const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
    info!("Waiting for controllers to shut down gracefully (timeout: {:?})", GRACEFUL_SHUTDOWN_TIMEOUT);

    // Create a future that waits for both controllers
    let both_controllers = async {
        tokio::try_join!(policy_handle, pod_handle)
    };

    // Attempt graceful shutdown with timeout
    match timeout(GRACEFUL_SHUTDOWN_TIMEOUT, both_controllers).await {
        Ok(Ok(_)) => {
            info!("Controllers shut down gracefully");
        }
        Ok(Err(e)) => {
            tracing::warn!("Controller join error: {}", e);
        }
        Err(_) => {
            info!("Graceful shutdown timeout exceeded, controllers did not respond to shutdown signal");
            // Stage 2: Forced termination would happen here, but handles are already consumed
            // The select! inside each controller should have caught the shutdown signal
        }
    }

    info!("Graceful shutdown complete");
    Ok(())
}
