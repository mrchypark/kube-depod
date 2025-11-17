use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes error: {0}")]
    KubeError(#[from] kube::error::Error),

    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("CEL compilation error: {0}")]
    CelCompilationError(String),

    #[error("CEL evaluation error: {0}")]
    CelEvaluationError(String),

    #[error("CRD validation error: {0}")]
    ValidationError(String),

    #[error("Custom error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
