use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid symbol: {0}")]
    InvalidSymbol(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid investment preferences: {0}")]
    InvalidPreferences(String),
    #[error("invalid trade decision: {0}")]
    InvalidDecision(String),
    #[error("LLM error: {0}")]
    Llm(String),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}
