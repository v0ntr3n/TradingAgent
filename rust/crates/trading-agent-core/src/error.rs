use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid symbol: {0}")]
    InvalidSymbol(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}
