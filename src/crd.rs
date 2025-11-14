use kube::CustomResource;
use serde::{Deserialize, Serialize};

/// Example Custom Resource Definition
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[kube(group = "example.com", version = "v1", kind = "Example")]
#[kube(namespaced)]
pub struct ExampleSpec {
    /// Name of the example resource
    pub name: String,
    /// Replicas
    pub replicas: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleStatus {
    pub ready: bool,
    pub message: Option<String>,
}
