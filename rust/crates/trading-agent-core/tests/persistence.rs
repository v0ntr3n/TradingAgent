use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use schemars::schema::RootSchema;
use serde_json::Value;
use trading_agent_core::{
    AgentState, ExternalReport, InvestmentPreferences, ModelTierConfig, PortfolioContext, Position,
    RunConfig, Symbol, SCHEMA_VERSION,
    agents::{AgentEvidence, Evidence},
    checkpoint::{CheckpointEnvelope, CheckpointStore, CompletedStage, RunSignature},
    events::{EventSink, RunEvent, WorkflowStage},
    workflow::{FinalRating, RunInput, WorkflowRunner},
};
use trading_agent_llm::{LlmClient, LlmError, LlmRequest, LlmResponse};

#[derive(Default)]
struct MemoryCheckpointStore {
    loaded: Mutex<Option<CheckpointEnvelope>>,
    saves: Mutex<Vec<CheckpointEnvelope>>,
    clears: AtomicUsize,
}

impl MemoryCheckpointStore {
    fn with_loaded(envelope: CheckpointEnvelope) -> Self {
        Self {
            loaded: Mutex::new(Some(envelope)),
            saves: Mutex::new(Vec::new()),
            clears: AtomicUsize::new(0),
        }
    }

    fn saved_stages(&self) -> Vec<CompletedStage> {
        self.saves
            .lock()
            .unwrap()
            .iter()
            .map(|envelope| envelope.completed_stage)
            .collect()
    }
}

impl CheckpointStore for MemoryCheckpointStore {
    fn load(&self) -> Result<Option<CheckpointEnvelope>, trading_agent_core::CoreError> {
        Ok(self.loaded.lock().unwrap().clone())
    }

    fn save(
        &self,
        envelope: &CheckpointEnvelope,
    ) -> Result<(), trading_agent_core::CoreError> {
        self.saves.lock().unwrap().push(envelope.clone());
        *self.loaded.lock().unwrap() = Some(envelope.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), trading_agent_core::CoreError> {
        self.clears.fetch_add(1, Ordering::SeqCst);
        *self.loaded.lock().unwrap() = None;
        Ok(())
    }
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<RunEvent>>,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: &RunEvent) -> Result<(), trading_agent_core::CoreError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[derive(Default)]
struct RecordingClient {
    labels: Mutex<Vec<String>>,
}

impl RecordingClient {
    fn labels(&self) -> Vec<String> {
        self.labels.lock().unwrap().clone()
    }
}

fn classify_prompt(text: &str) -> &'static str {
    if text.contains("TradingAgent market analyst") {
        "market"
    } else if text.contains("TradingAgent sentiment analyst") {
        "sentiment"
    } else if text.contains("TradingAgent news analyst") {
        "news"
    } else if text.contains("crypto fundamentals analyst")
        || text.contains("TradingAgent fundamentals analyst")
    {
        "fundamentals"
    } else if text.contains("Act as the bull researcher") {
        "bull"
    } else if text.contains("Act as the bear researcher") {
        "bear"
    } else if text.contains("Act as Research Manager") {
        "research_manager"
    } else if text.contains("Act as the trader") {
        "trader"
    } else if text.contains("Act as the aggressive risk analyst") {
        "risk_aggressive"
    } else if text.contains("Act as the neutral risk analyst") {
        "risk_neutral"
    } else if text.contains("Act as the conservative risk analyst") {
        "risk_conservative"
    } else if text.contains("Act as Portfolio Manager") {
        "portfolio_manager"
    } else {
        "other"
    }
}

#[async_trait]
impl LlmClient for RecordingClient {
    fn provider_id(&self) -> &str {
        "recording"
    }

    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let label = classify_prompt(&prompt).to_string();
        self.labels.lock().unwrap().push(label.clone());
        let content = match label.as_str() {
            "trader" => r#"{"action":"buy","reasoning":"checkpoint test","entry_price":"100","support":"95","resistance":"120","take_profit":"115","stop_loss":"90","position_sizing":{"description":"small","percent_of_portfolio":"5"}}"#.to_string(),
            "portfolio_manager" => "**Rating**: Hold\n\nFinal portfolio decision".into(),
            "research_manager" => "research plan".into(),
            "bull" => "bull case".into(),
            "bear" => "bear case".into(),
            "risk_aggressive" => "aggressive risk".into(),
            "risk_neutral" => "neutral risk".into(),
            "risk_conservative" => "conservative risk".into(),
            "fundamentals" => "fundamentals report".into(),
            "market" => "market report".into(),
            "sentiment" => "sentiment report".into(),
            "news" => "news report".into(),
            _ => "analysis".into(),
        };
        Ok(LlmResponse { content })
    }

    async fn complete_json(
        &self,
        _request: LlmRequest,
        _schema: &RootSchema,
    ) -> Result<Value, LlmError> {
        Err(LlmError::Transport("unused".into()))
    }
}

fn base_input() -> RunInput {
    let symbol = Symbol::parse("BTC-USD").unwrap();
    let mut state = AgentState::new(
        symbol.clone(),
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
    );
    state.instrument_context = Some("perpetual futures".into());
    state.investment_preferences = Some(
        InvestmentPreferences::from_json_str(
            r#"{"venue":"futures","allow_long":true,"allow_short":true,"style":"aggressive","holding_horizon":"1-3 days","min_leverage":"25","max_leverage":"100","max_loss_pct":"50"}"#,
        )
        .unwrap(),
    );
    state.external_reports.push(ExternalReport {
        title: Some("caller research".into()),
        source: Some("manual".into()),
        content: "outside thesis".into(),
    });
    state.portfolio = Some(PortfolioContext {
        cash: Some(Decimal::new(10_000, 0)),
        currency: Some("USD".into()),
        positions: vec![Position {
            symbol,
            quantity: Decimal::new(1, 0),
            average_price: Some(Decimal::new(60_000, 0)),
        }],
    });
    state.past_context = Some("prior lesson".into());

    RunInput {
        state,
        evidence: AgentEvidence {
            market: Evidence::Available("market evidence".into()),
            sentiment: Evidence::Available("sentiment evidence".into()),
            news: Evidence::Available("news evidence".into()),
            fundamentals: Evidence::Available("fundamentals evidence".into()),
        },
        config: RunConfig {
            quick: ModelTierConfig {
                provider: "openai_compatible".into(),
                model: "quick-model".into(),
                base_url: Some(
                    "https://user:pass@example.com/v1?api_key=query-secret&region=us".into(),
                ),
                api_key: Some("api-secret".into()),
            },
            deep: ModelTierConfig {
                provider: "anthropic".into(),
                model: "deep-model".into(),
                base_url: None,
                api_key: Some("deep-secret".into()),
            },
            output_language: "English".into(),
            analysts: vec!["market".into()],
            max_debate_rounds: 1,
            max_risk_rounds: 1,
        },
    }
}

#[test]
fn run_signature_covers_behavioral_inputs_and_safe_metadata_redacts_credentials() {
    let base = base_input();
    let signature = RunSignature::from_inputs(&base);
    let metadata = RunSignature::safe_metadata(&base);
    let persisted = serde_json::to_string(&metadata).unwrap();
    for secret in ["api-secret", "deep-secret", "query-secret", "user:pass"] {
        assert!(!persisted.contains(secret), "safe metadata leaked {secret}");
    }
    assert!(persisted.contains("<redacted>"));

    let mut cases = Vec::new();

    let mut value = base.clone();
    value.config.analysts.push("news".into());
    cases.push(value);

    let mut value = base.clone();
    value.config.max_debate_rounds = 2;
    cases.push(value);

    let mut value = base.clone();
    value.config.quick.provider = "google".into();
    cases.push(value);

    let mut value = base.clone();
    value.config.quick.model = "other-model".into();
    cases.push(value);

    let mut value = base.clone();
    value.config.quick.base_url = Some("https://other.example/v1".into());
    cases.push(value);

    let mut value = base.clone();
    value.config.output_language = "Thai".into();
    cases.push(value);

    let mut value = base.clone();
    value.state.portfolio.as_mut().unwrap().cash = Some(Decimal::new(9_000, 0));
    cases.push(value);

    let mut value = base.clone();
    value.state.investment_preferences.as_mut().unwrap().free_form = Some("prefer momentum".into());
    cases.push(value);

    let mut value = base.clone();
    value.state.external_reports.push(ExternalReport {
        title: None,
        source: Some("second".into()),
        content: "different evidence".into(),
    });
    cases.push(value);

    let mut value = base.clone();
    value.evidence.market = Evidence::Available("changed market evidence".into());
    cases.push(value);

    for changed in cases {
        assert_ne!(signature, RunSignature::from_inputs(&changed));
    }
}

#[tokio::test]
async fn unsupported_checkpoint_schema_is_rejected_before_any_agent_runs() {
    let input = base_input();
    let signature = RunSignature::from_inputs(&input);
    let envelope = CheckpointEnvelope {
        schema_version: SCHEMA_VERSION + 1,
        signature,
        completed_stage: CompletedStage::Research,
        state: input.state.clone(),
    };
    let store = Arc::new(MemoryCheckpointStore::with_loaded(envelope));
    let client = Arc::new(RecordingClient::default());
    let runner = WorkflowRunner::new(client.clone(), client.clone()).with_checkpoint_store(store);

    let error = runner.run(input).await.unwrap_err();
    assert!(error.to_string().contains("checkpoint schema"));
    assert!(client.labels().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workflow_saves_only_stable_boundaries_emits_events_and_clears_after_success() {
    let input = base_input();
    let store = Arc::new(MemoryCheckpointStore::default());
    let sink = Arc::new(RecordingSink::default());
    let client = Arc::new(RecordingClient::default());
    let runner = WorkflowRunner::new(client.clone(), client)
        .with_checkpoint_store(store.clone())
        .with_event_sink(sink.clone());

    let result = runner.run(input).await.unwrap();
    assert_eq!(result.rating, FinalRating::Hold);
    assert_eq!(
        store.saved_stages(),
        vec![
            CompletedStage::Analysts,
            CompletedStage::Research,
            CompletedStage::Trader,
        ]
    );
    assert_eq!(store.clears.load(Ordering::SeqCst), 1);
    assert!(store.loaded.lock().unwrap().is_none());

    let events = sink.events.lock().unwrap();
    assert!(matches!(events.first(), Some(RunEvent::Started { .. })));
    let stages = events
        .iter()
        .filter_map(|event| match event {
            RunEvent::SectionCompleted { stage, .. } => Some(*stage),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        vec![
            WorkflowStage::Analysts,
            WorkflowStage::Research,
            WorkflowStage::Trader,
            WorkflowStage::Risk,
            WorkflowStage::Portfolio,
        ]
    );
    assert!(matches!(events.last(), Some(RunEvent::Completed { rating: FinalRating::Hold })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn matching_checkpoint_resumes_after_research_but_stale_signature_restarts_from_analysts() {
    let input = base_input();
    let signature = RunSignature::from_inputs(&input);
    let mut checkpoint_state = input.state.clone();
    checkpoint_state.market_report = Some("checkpointed market".into());
    checkpoint_state.research_plan = Some("checkpointed research".into());
    let envelope = CheckpointEnvelope {
        schema_version: SCHEMA_VERSION,
        signature,
        completed_stage: CompletedStage::Research,
        state: checkpoint_state,
    };

    let store = Arc::new(MemoryCheckpointStore::with_loaded(envelope.clone()));
    let client = Arc::new(RecordingClient::default());
    let runner = WorkflowRunner::new(client.clone(), client.clone()).with_checkpoint_store(store);
    runner.run(input.clone()).await.unwrap();
    let labels = client.labels();
    assert!(!labels.iter().any(|label| label == "market"));
    assert!(!labels.iter().any(|label| label == "bull" || label == "bear" || label == "research_manager"));
    assert!(labels.iter().any(|label| label == "trader"));

    let stale_store = Arc::new(MemoryCheckpointStore::with_loaded(envelope));
    let stale_client = Arc::new(RecordingClient::default());
    let mut changed = input;
    changed.config.output_language = "Thai".into();
    let stale_runner = WorkflowRunner::new(stale_client.clone(), stale_client.clone())
        .with_checkpoint_store(stale_store.clone());
    stale_runner.run(changed).await.unwrap();
    let labels = stale_client.labels();
    assert!(labels.iter().any(|label| label == "market"));
    assert!(labels.iter().any(|label| label == "research_manager"));
    assert_eq!(stale_store.clears.load(Ordering::SeqCst), 2);
}
