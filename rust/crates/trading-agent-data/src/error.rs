use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DataError {
    #[error("invalid vendor configuration: {0}")]
    InvalidConfig(String),
    #[error("no vendor chain configured for {0}")]
    MissingChain(String),
    #[error("{source}: {message}")]
    Source { source: String, message: String },
}
