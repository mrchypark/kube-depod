use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Router,
};
use std::sync::Arc;
use tracing::info;

use crate::metrics::Metrics;

/// Metrics server state
#[derive(Clone)]
pub struct ServerState {
    pub metrics: Arc<Metrics>,
}

/// Prometheus metrics endpoint handler
pub async fn metrics_handler(State(state): State<ServerState>) -> (StatusCode, String) {
    let output = state.metrics.prometheus_format();
    (StatusCode::OK, output)
}

/// Health check endpoint
pub async fn health_handler() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}

/// Start metrics HTTP server
pub async fn start_server(
    metrics: Arc<Metrics>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ServerState { metrics };

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Metrics server listening on port {}", port);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let (status, body) = health_handler().await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "OK");
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let metrics = Arc::new(Metrics::new());
        metrics.increment_pods_evaluated();
        metrics.increment_pods_deleted();

        let state = ServerState { metrics };
        let (status, body) = metrics_handler(State(state)).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("kube_depod_pods_evaluated_total"));
        assert!(body.contains("1"));
    }
}
