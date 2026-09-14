use thiserror::Error;

#[derive(Debug, Error)]
pub enum DipError {
    #[error("adapter not found: {0}")]
    AdapterNotFound(String),

    #[error("no route from {from} to {to}")]
    NoRoute { from: String, to: String },

    #[error("envelope validation failed: {0}")]
    ValidationFailed(String),

    #[error("signature verification failed")]
    SignatureInvalid,

    #[error("network error: {0}")]
    Network(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("identity not found: {0}")]
    IdentityNotFound(String),
}
