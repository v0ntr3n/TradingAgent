use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use schemars::schema::RootSchema;
use serde_json::Value;
use tempfile::tempdir;
use trading_agent_core::{
    AgentState, ModelTierConfig, RunConfig, Symbol,
    agents::{AgentEvidence, Evidence},
    memory::{
        DecisionMemory, DecisionRecord, JsonDecisionMemory, SettlementHook, SettlementOutcome,
    },
    workflow::{FinalRating, RunInput, WorkflowRunner},
};
use trading_agent_llm::{LlmClient, LlmError, LlmRequest, LlmResponse};

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn decision(ticker: &str, date: NaiveDate, rating: FinalRating) -> DecisionRecord {
    DecisionRecord {
        ticker: ticker.into(),
        trade_date: date,
        final_decision: format!("decision for {ticker}"),
        rating,
        resolved: None,
    }
}

#[test]
fn new_decisions_are_pending_and_settlement_is_point_in_time_safe() {
    let temp = tempdir().unwrap();
    let memory = JsonDecisionMemory::new(temp.path().join("memory.json"));

    memory
        .store_decision(decision("AAPL", day(2026, 9, 1), FinalRating::Buy))
        .unwrap();
    assert_eq!(memory.pending().unwrap().len(), 1);

    let settled = memory
        .settle(
            "AAPL",
            day(2026, 9, 1),
            SettlementOutcome {
                raw_return: 0.10,
                alpha_return: 0.04,
                holding_days: 5,
                resolution_date: day(2026, 9, 8),
                reflection: "Momentum confirmation mattered.".into(),
            },
        )
        .unwrap();
    assert!(settled);
    assert!(memory.pending().unwrap().is_empty());

    assert!(
        memory
            .context_as_of("AAPL", Some(day(2026, 9, 7)))
            .unwrap()
            .is_empty()
    );
    let visible = memory.context_as_of("AAPL", Some(day(2026, 9, 8))).unwrap();
    assert!(visible.contains("decision for AAPL"));
    assert!(visible.contains("Momentum confirmation mattered."));
}

#[test]
fn same_ticker_lessons_are_prioritized_before_cross_ticker_lessons() {
    let temp = tempdir().unwrap();
    let memory = JsonDecisionMemory::new(temp.path().join("memory.json"));

    for (ticker, date, reflection) in [
        ("MSFT", day(2026, 8, 1), "cross one"),
        ("NVDA", day(2026, 8, 2), "cross two"),
        ("AAPL", day(2026, 8, 3), "same ticker lesson"),
        ("TSLA", day(2026, 8, 4), "cross three"),
        ("AMZN", day(2026, 8, 5), "cross four"),
    ] {
        memory
            .store_decision(decision(ticker, date, FinalRating::Hold))
            .unwrap();
        memory
            .settle(
                ticker,
                date,
                SettlementOutcome {
                    raw_return: 0.01,
                    alpha_return: 0.0,
                    holding_days: 5,
                    resolution_date: date.succ_opt().unwrap(),
                    reflection: reflection.into(),
                },
            )
            .unwrap();
    }

    let context = memory.context_as_of("AAPL", None).unwrap();
    let same = context.find("same ticker lesson").unwrap();
    let cross = context.find("Recent cross-ticker lessons").unwrap();
    assert!(same < cross);
    assert!(context.contains("cross four"));
}

struct FailingSettler;

#[async_trait]
impl SettlementHook for FailingSettler {
    async fn settle_pending(
        &self,
        _memory: &dyn DecisionMemory,
    ) -> Result<(), trading_agent_core::CoreError> {
        Err(trading_agent_core::CoreError::Persistence(
            "temporary price failure".into(),
        ))
    }
}

#[derive(Default)]
struct TestClient {
    calls: Mutex<Vec<String>>,
}

#[async_trait]
impl LlmClient for TestClient {
    fn provider_id(&self) -> &str {
        "test"
    }

    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let prompt = request
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let content = if prompt.contains("Act as the trader") {
            r#"{"action":"buy","reasoning":"memory test","entry_price":"100","support":"95","resistance":"120","take_profit":"115","stop_loss":"90","position_sizing":{"description":"small","percent_of_portfolio":"5"}}"#.to_string()
        } else if prompt.contains("Act as Portfolio Manager") {
            "**Rating**: Buy\n\nfinal".into()
        } else if prompt.contains("Act as Research Manager") {
            "research".into()
        } else {
            "analysis".into()
        };
        self.calls.lock().unwrap().push(prompt);
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

fn workflow_input() -> RunInput {
    RunInput {
        state: AgentState::new(Symbol::parse("AAPL").unwrap(), day(2026, 10, 6)),
        evidence: AgentEvidence {
            market: Evidence::Available("market".into()),
            sentiment: Evidence::Available("sentiment".into()),
            news: Evidence::Available("news".into()),
            fundamentals: Evidence::Available("fundamentals".into()),
        },
        config: RunConfig {
            quick: ModelTierConfig {
                provider: "test".into(),
                model: "quick".into(),
                base_url: None,
                api_key: None,
            },
            deep: ModelTierConfig {
                provider: "test".into(),
                model: "deep".into(),
                base_url: None,
                api_key: None,
            },
            output_language: "English".into(),
            analysts: vec!["market".into()],
            max_debate_rounds: 1,
            max_risk_rounds: 1,
        },
    }
}

#[tokio::test]
async fn settlement_failure_does_not_kill_run_and_pending_decisions_remain_pending() {
    let temp = tempdir().unwrap();
    let memory = Arc::new(JsonDecisionMemory::new(temp.path().join("memory.json")));
    memory
        .store_decision(decision("AAPL", day(2026, 9, 1), FinalRating::Hold))
        .unwrap();

    let client = Arc::new(TestClient::default());
    let runner = WorkflowRunner::new(client.clone(), client)
        .with_decision_memory(memory.clone())
        .with_settlement_hook(Arc::new(FailingSettler));

    let result = runner.run(workflow_input()).await.unwrap();
    assert_eq!(result.rating, FinalRating::Buy);

    let pending = memory.pending().unwrap();
    assert!(
        pending
            .iter()
            .any(|record| record.trade_date == day(2026, 9, 1))
    );
    assert!(
        pending
            .iter()
            .any(|record| record.trade_date == day(2026, 10, 6))
    );
}
