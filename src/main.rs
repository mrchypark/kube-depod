use anyhow::Result;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher::{watcher, Config};
use kube::{Api, Client};
use kube_depod::controller::{load_policies, reconcile_pod};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting kube-depod operator");

    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

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
                            if let Err(e) = reconcile_pod(pod, &policies, &client).await {
                                tracing::warn!("Reconciliation failed: {}", e);
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
                    tracing::error!("Watch error: {}", e);
                    // Continue watching
                }
            }
        }

        info!("Pod watcher ended, restarting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
