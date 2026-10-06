use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LlmError {
    #[error("unsupported LLM provider: {0}")]
    UnsupportedProvider(String),
    #[error("invalid LLM configuration: {0}")]
    InvalidConfig(String),
    #[error("LLM transport failed: {0}")]
    Transport(String),
    #[error("structured output error: {0}")]
    Structured(String),
    #[error("search research is current-only and cannot be used for historical date {as_of}")]
    HistoricalSearchRefused { as_of: NaiveDate },
}

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub max_retries: u32,
}

impl ProviderConfig {
    pub fn safe_display(&self) -> String {
        let safe_url = self.base_url.as_deref().map(redact_url);
        json!({
            "provider": self.provider,
            "model": self.model,
            "base_url": safe_url,
            "api_key": self.api_key.as_ref().map(|_| "<redacted>"),
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "max_retries": self.max_retries,
        })
        .to_string()
    }
}

fn redact_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return "<redacted-invalid-url>".into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);

    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let key_string = key.into_owned();
            let value_string = value.into_owned();
            let lowered = key_string.to_ascii_lowercase();
            let sensitive = lowered.contains("key")
                || lowered.contains("token")
                || lowered.contains("secret")
                || lowered.contains("password")
                || lowered == "sig"
                || lowered == "signature";
            (key_string, if sensitive { "<redacted>".into() } else { value_string })
        })
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

impl LlmMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: LlmRole::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: LlmRole::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: LlmRole::Assistant, content: content.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LlmRequest {
    pub messages: Vec<LlmMessage>,
    pub enable_search: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

impl LlmRequest {
    pub fn new(messages: Vec<LlmMessage>) -> Self {
        Self { messages, enable_search: false, temperature: None, max_tokens: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmResponse {
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LlmHttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub max_retries: u32,
}
