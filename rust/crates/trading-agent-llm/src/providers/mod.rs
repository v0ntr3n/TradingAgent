use std::sync::Arc;

use async_trait::async_trait;
use schemars::schema::RootSchema;
use serde_json::{Value, json};

use crate::{
    LlmClient, LlmError, LlmHttpRequest, LlmRequest, LlmResponse, LlmRole, LlmTransport,
    ProviderConfig,
};

pub(crate) mod anthropic;
pub(crate) mod azure;
pub(crate) mod bedrock;
pub(crate) mod google;
pub(crate) mod openai_compatible;

#[derive(Clone, Copy)]
pub(crate) enum Flavor {
    OpenAiCompatible,
    Anthropic,
    Google,
    Azure,
    Bedrock,
}

pub(crate) struct HttpProviderClient {
    adapter_id: &'static str,
    flavor: Flavor,
    config: ProviderConfig,
    transport: Arc<dyn LlmTransport>,
}

impl HttpProviderClient {
    pub(crate) fn new(
        adapter_id: &'static str,
        flavor: Flavor,
        mut config: ProviderConfig,
        default_base_url: &str,
        transport: Arc<dyn LlmTransport>,
    ) -> Result<Self, LlmError> {
        if config.model.trim().is_empty() {
            return Err(LlmError::InvalidConfig("model cannot be empty".into()));
        }
        if config.base_url.as_deref().map(str::trim).unwrap_or("").is_empty() {
            config.base_url = Some(default_base_url.into());
        }
        Ok(Self { adapter_id, flavor, config, transport })
    }

    fn base_url(&self) -> &str {
        self.config.base_url.as_deref().expect("base URL normalized in constructor").trim_end_matches('/')
    }

    fn sampling(&self, request: &LlmRequest) -> (Option<f64>, Option<u32>) {
        (request.temperature.or(self.config.temperature), request.max_tokens.or(self.config.max_tokens))
    }

    fn messages_json(&self, request: &LlmRequest) -> Vec<Value> {
        request.messages.iter().map(|message| {
            let role = match message.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
            };
            json!({"role": role, "content": message.content})
        }).collect()
    }

    fn http_request(&self, request: &LlmRequest, schema: Option<&RootSchema>) -> Result<LlmHttpRequest, LlmError> {
        let (temperature, max_tokens) = self.sampling(request);
        let api_key = self.config.api_key.as_deref().unwrap_or("");
        let mut headers = vec![("content-type".into(), "application/json".into())];
        let (url, mut body) = match self.flavor {
            Flavor::OpenAiCompatible => {
                if !api_key.is_empty() { headers.push(("authorization".into(), format!("Bearer {api_key}"))); }
                (format!("{}/chat/completions", self.base_url()), json!({
                    "model": self.config.model,
                    "messages": self.messages_json(request),
                }))
            }
            Flavor::Azure => {
                if !api_key.is_empty() { headers.push(("api-key".into(), api_key.into())); }
                (format!("{}/openai/deployments/{}/chat/completions?api-version=2024-10-21", self.base_url(), self.config.model), json!({
                    "messages": self.messages_json(request),
                }))
            }
            Flavor::Anthropic => {
                if !api_key.is_empty() { headers.push(("x-api-key".into(), api_key.into())); }
                headers.push(("anthropic-version".into(), "2023-06-01".into()));
                let system = request.messages.iter().filter(|m| m.role == LlmRole::System).map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
                let messages = request.messages.iter().filter(|m| m.role != LlmRole::System).map(|m| {
                    json!({"role": if m.role == LlmRole::Assistant {"assistant"} else {"user"}, "content": m.content})
                }).collect::<Vec<_>>();
                (format!("{}/v1/messages", self.base_url()), json!({"model": self.config.model, "system": system, "messages": messages}))
            }
            Flavor::Google => {
                let url = if api_key.is_empty() {
                    format!("{}/v1beta/models/{}:generateContent", self.base_url(), self.config.model)
                } else {
                    format!("{}/v1beta/models/{}:generateContent?key={}", self.base_url(), self.config.model, api_key)
                };
                let contents = request.messages.iter().filter(|m| m.role != LlmRole::System).map(|m| {
                    json!({"role": if m.role == LlmRole::Assistant {"model"} else {"user"}, "parts":[{"text":m.content}]})
                }).collect::<Vec<_>>();
                (url, json!({"contents": contents}))
            }
            Flavor::Bedrock => {
                if !api_key.is_empty() { headers.push(("authorization".into(), format!("Bearer {api_key}"))); }
                let messages = request.messages.iter().filter(|m| m.role != LlmRole::System).map(|m| {
                    json!({"role": if m.role == LlmRole::Assistant {"assistant"} else {"user"}, "content":[{"text":m.content}]})
                }).collect::<Vec<_>>();
                (format!("{}/model/{}/converse", self.base_url(), self.config.model), json!({"messages": messages}))
            }
        };

        if let Some(temp) = temperature {
            match self.flavor {
                Flavor::Google => body["generationConfig"]["temperature"] = json!(temp),
                Flavor::Bedrock => body["inferenceConfig"]["temperature"] = json!(temp),
                _ => body["temperature"] = json!(temp),
            }
        }
        if let Some(tokens) = max_tokens {
            match self.flavor {
                Flavor::Anthropic => body["max_tokens"] = json!(tokens),
                Flavor::Google => body["generationConfig"]["maxOutputTokens"] = json!(tokens),
                Flavor::Bedrock => body["inferenceConfig"]["maxTokens"] = json!(tokens),
                _ => body["max_tokens"] = json!(tokens),
            }
        } else if matches!(self.flavor, Flavor::Anthropic) {
            body["max_tokens"] = json!(4096);
        }
        if request.enable_search {
            body["enable_search"] = json!(true);
        }
        if let Some(schema) = schema {
            match self.flavor {
                Flavor::OpenAiCompatible | Flavor::Azure => {
                    body["response_format"] = json!({"type":"json_schema","json_schema":{"name":"structured_response","schema":schema}});
                }
                Flavor::Google => {
                    body["generationConfig"]["responseMimeType"] = json!("application/json");
                    body["generationConfig"]["responseSchema"] = serde_json::to_value(schema).map_err(|e| LlmError::Structured(e.to_string()))?;
                }
                Flavor::Anthropic | Flavor::Bedrock => {
                    body["structured_output_schema"] = serde_json::to_value(schema).map_err(|e| LlmError::Structured(e.to_string()))?;
                }
            }
        }
        Ok(LlmHttpRequest { url, headers, body, max_retries: self.config.max_retries })
    }

    fn extract_text(&self, value: &Value) -> Result<String, LlmError> {
        let text = match self.flavor {
            Flavor::OpenAiCompatible | Flavor::Azure => value.pointer("/choices/0/message/content").and_then(Value::as_str),
            Flavor::Anthropic => value.pointer("/content/0/text").and_then(Value::as_str),
            Flavor::Google => value.pointer("/candidates/0/content/parts/0/text").and_then(Value::as_str),
            Flavor::Bedrock => value.pointer("/output/message/content/0/text").and_then(Value::as_str),
        };
        text.map(str::to_owned).ok_or_else(|| LlmError::Transport("provider response did not contain text".into()))
    }
}

#[async_trait]
impl LlmClient for HttpProviderClient {
    fn provider_id(&self) -> &str { self.adapter_id }

    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let response = self.transport.execute(self.http_request(&request, None)?).await?;
        Ok(LlmResponse { content: self.extract_text(&response)? })
    }

    async fn complete_json(&self, request: LlmRequest, schema: &RootSchema) -> Result<Value, LlmError> {
        let response = self.transport.execute(self.http_request(&request, Some(schema))?).await?;
        let text = self.extract_text(&response)?;
        serde_json::from_str(&text).map_err(|error| LlmError::Structured(error.to_string()))
    }
}
