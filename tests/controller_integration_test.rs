//! Full controller integration tests
//!
//! This test suite requires a running Kubernetes cluster.
//! It uses `k8s-test` to spin up a temporary `k3s` cluster.
//!
//! To run these tests:
//! `cargo test --test controller_integration_test -- --nocapture`

use anyhow::Result;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::Api;
use k8s_test::{TestCluster, TestSetup};
use std::time::Duration;
use tokio::time::timeout;

/// Sets up a temporary k3s cluster and returns a TestSetup object.
async fn setup_cluster() -> Result<TestSetup> {
    let mut cluster = TestCluster::new();
    cluster.start().await?;
    let setup = cluster.setup().await?;
    Ok(setup)
}

#[tokio::test]
async fn test_crd_installation() -> Result<()> {
    // 1. Setup the test cluster
    let setup = setup_cluster().await?;
    let client = setup.client();

    // 2. Apply the DepodPolicy CRD manifest
    let crd_api: Api<CustomResourceDefinition> = Api::all(client.clone());
    let crd_manifest = std::fs::read_to_string("manifests/crd.yaml")?;

    // Apply the CRD using kubectl apply
    setup.kubectl_apply(&crd_manifest).await?;

    // 3. Wait for the CRD to be established
    let wait_for_crd = timeout(
        Duration::from_secs(30),
        k8s_test::wait::until_crd_is_established(crd_api, "depodpolicies.kube-depod.io"),
    )
    .await;

    assert!(
        wait_for_crd.is_ok(),
        "Timed out waiting for DepodPolicy CRD to be established"
    );

    Ok(())
}
