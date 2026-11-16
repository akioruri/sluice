use thiserror::Error;

#[derive(Debug, Error)]
pub enum SluiceError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("provider not found: {0}")]
    ProviderNotFound(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("upstream error: {0}")]
    UpstreamError(String),

    #[error("crypto error: {0}")]
    CryptoError(String),

    #[error("database error: {0}")]
    DatabaseError(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, SluiceError>;
