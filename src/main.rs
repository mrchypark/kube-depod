use anyhow::Result;
use kube::Client;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Kubernetes operator");

    let _client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // TODO: Implement reconciliation loop

    Ok(())
}
