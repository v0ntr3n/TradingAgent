use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::NaiveDate;
use schemars::schema::RootSchema;
use serde_json::Value;
use trading_agent_core::{
    AgentState, ModelTierConfig, RunConfig, Symbol,
    agents::{AgentEvidence, Evidence},
    workflow::{FinalRating, RunInput, WorkflowRunner},
};
use trading_agent_llm::{LlmClient, LlmError, LlmRequest, LlmResponse};

struct InstrumentedClient {
    final_response: String,
    labels: Mutex<Vec<String>>,
    active_analysts: AtomicUsize,
    max_active_analysts: AtomicUsize,
}

impl InstrumentedClient {
    fn new(final_response: impl Into<String>) -> Self {
        Self {
            final_response: final_response.into(),
            labels: Mutex::new(Vec::new()),
            active_analysts: AtomicUsize::new(0),
            max_active_analysts: AtomicUsize::new(0),
        }
    }

    fn labels(&self) -> Vec<String> {
        self.labels.lock().unwrap().clone()
    }

    fn max_active_analysts(&self) -> usize {
        self.max_active_analysts.load(Ordering::SeqCst)
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
impl LlmClient for InstrumentedClient {
    fn provider_id(&self) -> &str {
        "instrumented"
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

        if matches!(
            label.as_str(),
            "market" | "sentiment" | "news" | "fundamentals"
        ) {
            let active = self.active_analysts.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_analysts.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.active_analysts.fetch_sub(1, Ordering::SeqCst);
        }

        let content = match label.as_str() {
            "trader" => r#"{"action":"buy","reasoning":"workflow test","entry_price":"100","support":"95","resistance":"120","take_profit":"115","stop_loss":"90","position_sizing":{"description":"small","percent_of_portfolio":"5"}}"#.to_string(),
            "portfolio_manager" => self.final_response.clone(),
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

fn crypto_state() -> AgentState {
    AgentState::new(
        Symbol::parse("BTC-USD").unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
    )
}

fn evidence() -> AgentEvidence {
    AgentEvidence {
        market: Evidence::Available("market evidence".into()),
        sentiment: Evidence::Available("sentiment evidence".into()),
        news: Evidence::Available("news evidence".into()),
        fundamentals: Evidence::Available("crypto fundamentals evidence".into()),
    }
}

fn config(analysts: &[&str], debate_rounds: u32, risk_rounds: u32) -> RunConfig {
    let tier = ModelTierConfig {
        provider: "test".into(),
        model: "test-model".into(),
        base_url: None,
        api_key: None,
    };
    RunConfig {
        quick: tier.clone(),
        deep: tier,
        output_language: "English".into(),
        analysts: analysts.iter().map(|value| (*value).to_string()).collect(),
        max_debate_rounds: debate_rounds,
        max_risk_rounds: risk_rounds,
    }
}

fn input(analysts: &[&str], debate_rounds: u32, risk_rounds: u32) -> RunInput {
    RunInput {
        state: crypto_state(),
        evidence: evidence(),
        config: config(analysts, debate_rounds, risk_rounds),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn analysts_run_concurrently_and_only_selected_analysts_execute() {
    let client = Arc::new(InstrumentedClient::new("**Rating**: Hold"));
    let runner = WorkflowRunner::new(client.clone(), client.clone());

    let result = runner.run(input(&["market", "news"], 0, 0)).await.unwrap();

    assert!(
        client.max_active_analysts() >= 2,
        "selected analysts did not overlap"
    );
    let labels = client.labels();
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "market")
            .count(),
        1
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "news")
            .count(),
        1
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "sentiment")
            .count(),
        0
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "fundamentals")
            .count(),
        0
    );
    assert_eq!(result.state.market_report.as_deref(), Some("market report"));
    assert_eq!(result.state.news_report.as_deref(), Some("news report"));
    assert!(result.state.sentiment_report.is_none());
    assert!(result.state.fundamentals_report.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn debate_rounds_risk_order_and_crypto_fundamentals_are_deterministic() {
    let client = Arc::new(InstrumentedClient::new("**Rating**: Overweight"));
    let runner = WorkflowRunner::new(client.clone(), client.clone());

    let result = runner
        .run(input(
            &["market", "sentiment", "news", "fundamentals"],
            2,
            2,
        ))
        .await
        .unwrap();

    let labels = client.labels();
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "bull")
            .count(),
        2
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "bear")
            .count(),
        2
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "fundamentals")
            .count(),
        1
    );

    let risk_order = labels
        .iter()
        .filter(|label| label.starts_with("risk_"))
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        risk_order,
        vec![
            "risk_aggressive",
            "risk_neutral",
            "risk_conservative",
            "risk_aggressive",
            "risk_neutral",
            "risk_conservative",
        ]
    );
    assert_eq!(result.rating, FinalRating::Overweight);
    assert_eq!(
        result.state.fundamentals_report.as_deref(),
        Some("fundamentals report")
    );
    assert!(result.state.trader_proposal.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn final_rating_maps_five_tiers_and_malformed_or_missing_values_to_review() {
    let cases = [
        ("**Rating**: Buy", FinalRating::Buy),
        ("**Rating**: Overweight", FinalRating::Overweight),
        ("**Rating**: Hold", FinalRating::Hold),
        ("**Rating**: Underweight", FinalRating::Underweight),
        ("**Rating**: Sell", FinalRating::Sell),
        ("**Rating**: Strong Buy", FinalRating::Review),
        ("No explicit rating was produced.", FinalRating::Review),
    ];

    for (manager_output, expected) in cases {
        let client = Arc::new(InstrumentedClient::new(manager_output));
        let runner = WorkflowRunner::new(client.clone(), client);
        let result = runner.run(input(&["market"], 0, 0)).await.unwrap();
        assert_eq!(result.rating, expected, "manager output: {manager_output}");
    }
}
