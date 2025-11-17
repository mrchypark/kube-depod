use anyhow::Result;
use arc_swap::ArcSwap;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};
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
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting kube-depod operator");

    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // Read operator Pod name from Downward API environment variable
    let operator_pod_name = std::env::var("OPERATOR_POD_NAME")
        .unwrap_or_else(|_| "kube-depod-unknown".to_string());
    info!("Operator Pod Name: {}", operator_pod_name);

    // Load periodic resync (cron check) configuration
    let periodic_resync_interval = if std::env::var("RESYNC_ENABLE")
        .unwrap_or_else(|_| "true".to_string())
        .eq_ignore_ascii_case("true")
    {
        let interval_seconds = std::env::var("RESYNC_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "3600".to_string())
            .parse::<u64>()
            .unwrap_or(3600);

        info!(
            "Periodic Resync (Cron Check) enabled. Interval: {} seconds",
            interval_seconds
        );
        Some(Duration::from_secs(interval_seconds))
    } else {
        info!("Periodic Resync (Cron Check) is disabled.");
        None
    };

    // Create metrics instance
    let metrics = Arc::new(Metrics::new());
    let metrics_clone = metrics.clone();

    // Read rate limit from environment variable (default: 20)
    let rate_limit_per_minute = std::env::var("RATE_LIMIT_PER_MINUTE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(20);
    info!("Rate limit configured: {} deletes per minute", rate_limit_per_minute);
    let rate_limiter = Arc::new(RateLimiter::new(rate_limit_per_minute));

    // Read pod patch concurrency limit from environment variable (default: 10)
    let pod_patch_concurrency_limit = std::env::var("POD_PATCH_CONCURRENCY_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10);
    info!(
        "Pod patch concurrency limit configured: {} concurrent patch operations",
        pod_patch_concurrency_limit
    );

    // Read metrics port from environment variable (default: 8080)
    let metrics_port = std::env::var("METRICS_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);
    info!("Metrics server port configured: {}", metrics_port);

    // Start metrics HTTP server in background task
    tokio::spawn(async move {
        if let Err(e) = start_server(metrics_clone, metrics_port).await {
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

    // Create Context for shared state
    let ctx = Arc::new(Context {
        client: client.clone(),
        metrics: metrics.clone(),
        evaluator: Arc::new(CelEvaluator::new()),
        policies,
        rate_limiter,
        operator_pod_name: Arc::new(operator_pod_name),
        periodic_resync_interval,
        pod_patch_concurrency_limit,
    });

    // --- Start DepodPolicy controller ---
    let policy_api: Api<DepodPolicy> = Api::all(client.clone());
    let policy_ctx = ctx.clone();

    tokio::spawn(async move {
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

    info!("Controller finished");
    Ok(())
}
