use async_trait::async_trait;
use schemars::schema::RootSchema;
use serde_json::Value;

use crate::{LlmError, LlmHttpRequest, LlmRequest, LlmResponse};

#[async_trait]
pub trait LlmTransport: Send + Sync {
    async fn execute(&self, request: LlmHttpRequest) -> Result<Value, LlmError>;
}

#[derive(Clone, Default)]
pub struct ReqwestLlmTransport {
    client: reqwest::Client,
}

impl ReqwestLlmTransport {
    pub fn new() -> Self { Self::default() }
}

#[async_trait]
impl LlmTransport for ReqwestLlmTransport {
    async fn execute(&self, request: LlmHttpRequest) -> Result<Value, LlmError> {
        let attempts = request.max_retries.saturating_add(1);
        let mut last_error = None;
        for _ in 0..attempts {
            let mut builder = self.client.post(&request.url).json(&request.body);
            for (key, value) in &request.headers {
                builder = builder.header(key, value);
            }
            match builder.send().await {
                Ok(response) if response.status().is_success() => {
                    return response.json::<Value>().await.map_err(|_| {
                        LlmError::Transport("provider returned invalid JSON".into())
                    });
                }
                Ok(response) => {
                    last_error = Some(format!("provider returned HTTP {}", response.status()));
                }
                Err(error) => {
                    last_error = Some(if error.is_timeout() {
                        "provider request timed out".into()
                    } else {
                        "provider request failed".into()
                    });
                }
            }
        }
        Err(LlmError::Transport(
            last_error.unwrap_or_else(|| "provider request failed".into()),
        ))
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
    async fn complete_json(
        &self,
        request: LlmRequest,
        schema: &RootSchema,
    ) -> Result<Value, LlmError>;
}
