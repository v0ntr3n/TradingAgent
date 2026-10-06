use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use schemars::{JsonSchema, schema::RootSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use trading_agent_llm::{
    LlmClient, LlmError, LlmHttpRequest, LlmMessage, LlmRequest, LlmResponse, LlmTransport,
    ProviderConfig, ProviderRegistry, ResearchClient, ResearchRequest, SearchResearchClient,
    complete_structured,
};

#[derive(Clone)]
struct RecordingTransport {
    response: Value,
    requests: Arc<Mutex<Vec<LlmHttpRequest>>>,
}

impl RecordingTransport {
    fn openai_ok(content: &str) -> Self {
        Self {
            response: json!({"choices":[{"message":{"content":content}}]}),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl LlmTransport for RecordingTransport {
    async fn execute(&self, request: LlmHttpRequest) -> Result<Value, LlmError> {
        self.requests.lock().unwrap().push(request);
        Ok(self.response.clone())
    }
}

fn config(provider: &str, model: &str) -> ProviderConfig {
    ProviderConfig {
        provider: provider.into(),
        model: model.into(),
        base_url: Some("https://llm.example/v1".into()),
        api_key: Some("top-secret".into()),
        temperature: Some(0.2),
        max_tokens: Some(2048),
        max_retries: 3,
    }
}

#[test]
fn registry_selects_native_and_openai_compatible_provider_families() {
    let transport = Arc::new(RecordingTransport::openai_ok("ok"));
    let registry = ProviderRegistry::new(transport);
    let cases = [
        ("openai", "openai_compatible"),
        ("openai_compatible", "openai_compatible"),
        ("qwen", "openai_compatible"),
        ("dashscope", "openai_compatible"),
        ("gitee", "openai_compatible"),
        ("openrouter", "openai_compatible"),
        ("anthropic", "anthropic"),
        ("google", "google"),
        ("azure", "azure"),
        ("bedrock", "bedrock"),
    ];

    for (provider, adapter) in cases {
        let client = registry.create(&config(provider, "model-x")).unwrap();
        assert_eq!(client.provider_id(), adapter, "provider={provider}");
    }
    assert!(matches!(
        registry.create(&config("unknown-provider", "model-x")),
        Err(LlmError::UnsupportedProvider(_))
    ));
}

#[test]
fn quick_and_deep_tiers_can_use_different_provider_families() {
    let registry = ProviderRegistry::new(Arc::new(RecordingTransport::openai_ok("ok")));
    let quick = registry.create(&config("qwen", "qwen-turbo")).unwrap();
    let deep = registry
        .create(&config("anthropic", "claude-sonnet"))
        .unwrap();
    assert_eq!(quick.provider_id(), "openai_compatible");
    assert_eq!(deep.provider_id(), "anthropic");
}

#[tokio::test]
async fn openai_compatible_propagates_endpoint_sampling_limits_retries_and_auth() {
    let transport = RecordingTransport::openai_ok("answer");
    let inspect = transport.clone();
    let registry = ProviderRegistry::new(Arc::new(transport));
    let mut cfg = config("qwen", "qwen-plus");
    cfg.base_url = Some("https://dashscope.example/compatible-mode/v1".into());
    cfg.temperature = Some(0.35);
    cfg.max_tokens = Some(4096);
    cfg.max_retries = 5;
    let client = registry.create(&cfg).unwrap();

    let response = client
        .complete(LlmRequest::new(vec![LlmMessage::user("Analyze BTC")]))
        .await
        .unwrap();
    assert_eq!(response.content, "answer");

    let requests = inspect.requests.lock().unwrap();
    let sent = &requests[0];
    assert_eq!(
        sent.url,
        "https://dashscope.example/compatible-mode/v1/chat/completions"
    );
    assert_eq!(sent.max_retries, 5);
    assert!(
        sent.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v == "Bearer top-secret")
    );
    assert_eq!(sent.body["model"], "qwen-plus");
    assert_eq!(sent.body["temperature"], 0.35);
    assert_eq!(sent.body["max_tokens"], 4096);
    assert_eq!(sent.body["messages"][0]["content"], "Analyze BTC");
}

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct StructuredDecision {
    action: String,
    confidence: u8,
}

struct JsonClient {
    value: Value,
}

#[async_trait]
impl LlmClient for JsonClient {
    fn provider_id(&self) -> &str {
        "test"
    }

    async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
        Err(LlmError::Transport("not used".into()))
    }

    async fn complete_json(
        &self,
        _request: LlmRequest,
        _schema: &RootSchema,
    ) -> Result<Value, LlmError> {
        Ok(self.value.clone())
    }
}

#[tokio::test]
async fn structured_completion_deserializes_valid_shape_and_rejects_invalid_shape() {
    let request = LlmRequest::new(vec![LlmMessage::user("decide")]);
    let valid = JsonClient {
        value: json!({"action":"buy","confidence":87}),
    };
    let decision: StructuredDecision = complete_structured(&valid, request.clone()).await.unwrap();
    assert_eq!(
        decision,
        StructuredDecision {
            action: "buy".into(),
            confidence: 87
        }
    );

    let invalid = JsonClient {
        value: json!({"action":"buy","confidence":"high"}),
    };
    assert!(matches!(
        complete_structured::<StructuredDecision>(&invalid, request).await,
        Err(LlmError::Structured(_))
    ));
}

#[tokio::test]
async fn search_research_refuses_historical_queries_before_calling_the_llm() {
    let transport = RecordingTransport::openai_ok("current research");
    let inspect = transport.clone();
    let registry = ProviderRegistry::new(Arc::new(transport));
    let client = registry.create(&config("qwen", "qwen-plus")).unwrap();
    let research = SearchResearchClient::new(client, NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());

    let error = research
        .research(ResearchRequest {
            query: "BTC fundamentals".into(),
            as_of: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
        })
        .await
        .unwrap_err();
    assert!(matches!(error, LlmError::HistoricalSearchRefused { .. }));
    assert_eq!(inspect.count(), 0);
}

#[test]
fn safe_provider_display_redacts_keys_url_credentials_and_sensitive_query_values() {
    let mut cfg = config("openai_compatible", "model-x");
    cfg.base_url =
        Some("https://alice:password@llm.example/v1?api_key=endpoint-secret&region=us".into());
    cfg.api_key = Some("top-secret".into());

    let safe = cfg.safe_display();
    assert!(safe.contains("llm.example"));
    assert!(safe.contains("region=us"));
    for secret in ["alice", "password", "endpoint-secret", "top-secret"] {
        assert!(!safe.contains(secret), "leaked {secret}: {safe}");
    }
    assert!(safe.contains("%3Credacted%3E") || safe.contains("<redacted>"));
}
