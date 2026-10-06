use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use schemars::schema::RootSchema;
use serde_json::Value;
use trading_agent_core::{
    AgentState, ModelTierConfig, PortfolioContext, RunConfig, Symbol,
    agents::{AgentEvidence, Evidence},
    backtest::{
        BacktestDataSource, BacktestInput, BacktestPriceWindow, BacktestRequest, BacktestRunner,
    },
    workflow::{FinalRating, RunInput, WorkflowRunner},
};
use trading_agent_llm::{LlmClient, LlmError, LlmRequest, LlmResponse};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

#[derive(Default)]
struct BacktestClient {
    prompts: Mutex<Vec<String>>,
}

#[async_trait]
impl LlmClient for BacktestClient {
    fn provider_id(&self) -> &str {
        "backtest-test"
    }

    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let prompt = request
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.prompts.lock().unwrap().push(prompt.clone());
        let content = if prompt.contains("Act as the trader") {
            r#"{"action":"hold","reasoning":"fixture","entry_price":null,"support":null,"resistance":null,"take_profit":null,"stop_loss":null,"position_sizing":null}"#.into()
        } else if prompt.contains("Act as Portfolio Manager") {
            let rating = if prompt.contains("Trade date: 2026-10-01") {
                "Buy"
            } else if prompt.contains("Trade date: 2026-10-03") {
                "Sell"
            } else {
                "Hold"
            };
            format!("**Rating**: {rating}\n\nfixture decision")
        } else if prompt.contains("Act as Research Manager") {
            "fixture research".into()
        } else {
            "fixture report".into()
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

#[derive(Default)]
struct FixtureSource {
    calls: Mutex<Vec<String>>,
    wrong_date: bool,
}

impl FixtureSource {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl BacktestDataSource for FixtureSource {
    async fn run_input(
        &self,
        symbol: &Symbol,
        as_of: NaiveDate,
    ) -> Result<BacktestInput, trading_agent_core::CoreError> {
        self.calls.lock().unwrap().push(format!("input:{as_of}"));
        if as_of == day(2) {
            return Ok(BacktestInput::Unavailable {
                reason: "market holiday fixture".into(),
            });
        }
        let actual_date = if self.wrong_date {
            as_of.succ_opt().unwrap()
        } else {
            as_of
        };
        let mut state = AgentState::new(symbol.clone(), actual_date);
        state.portfolio = Some(PortfolioContext {
            cash: Some(Decimal::new(10_000, 0)),
            currency: Some("USD".into()),
            positions: vec![],
        });
        let tier = ModelTierConfig {
            provider: "test".into(),
            model: "test".into(),
            base_url: None,
            api_key: None,
        };
        Ok(BacktestInput::Available(Box::new(RunInput {
            state,
            evidence: AgentEvidence {
                market: Evidence::Available(format!("market as-of {as_of}")),
                sentiment: Evidence::Available(format!("sentiment as-of {as_of}")),
                news: Evidence::Available(format!("news as-of {as_of}")),
                fundamentals: Evidence::Available(format!("fundamentals as-of {as_of}")),
            },
            config: RunConfig {
                quick: tier.clone(),
                deep: tier,
                output_language: "English".into(),
                analysts: vec!["market".into()],
                max_debate_rounds: 0,
                max_risk_rounds: 0,
            },
        })))
    }

    async fn price_window(
        &self,
        _symbol: &Symbol,
        as_of: NaiveDate,
        holding_days: u32,
        benchmark: &Symbol,
    ) -> Result<Option<BacktestPriceWindow>, trading_agent_core::CoreError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("price:{as_of}:{holding_days}:{benchmark}"));
        let (exit, benchmark_exit) = match as_of.day() {
            1 => (Decimal::new(110, 0), Decimal::new(105, 0)),
            3 => (Decimal::new(90, 0), Decimal::new(102, 0)),
            _ => (Decimal::new(102, 0), Decimal::new(101, 0)),
        };
        Ok(Some(BacktestPriceWindow {
            entry_price: Decimal::new(100, 0),
            exit_price: exit,
            benchmark_entry_price: Decimal::new(100, 0),
            benchmark_exit_price: benchmark_exit,
            resolution_date: as_of
                .checked_add_days(chrono::Days::new(holding_days as u64))
                .unwrap(),
        }))
    }
}

use chrono::Datelike;

fn request(dates: Vec<NaiveDate>) -> BacktestRequest {
    BacktestRequest {
        symbols: vec![Symbol::parse("AAPL").unwrap()],
        dates,
        holding_days: 5,
        benchmark: Symbol::parse("SPY").unwrap(),
    }
}

#[tokio::test]
async fn dates_are_sorted_deduplicated_unavailable_days_are_isolated_and_portfolio_is_not_carried()
{
    let client = Arc::new(BacktestClient::default());
    let workflow = Arc::new(WorkflowRunner::new(client.clone(), client));
    let source = Arc::new(FixtureSource::default());
    let runner = BacktestRunner::new(workflow, source.clone());

    let result = runner
        .run(request(vec![day(3), day(1), day(2), day(1), day(4)]))
        .await
        .unwrap();

    assert_eq!(
        result.cells.iter().map(|c| c.date).collect::<Vec<_>>(),
        vec![day(1), day(2), day(3), day(4)]
    );
    assert!(
        result.cells[1]
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains("holiday")
    );
    assert!(result.cells[1].result.is_none());
    for cell in result.cells.iter().filter(|cell| cell.result.is_some()) {
        assert_eq!(
            cell.result
                .as_ref()
                .unwrap()
                .state
                .portfolio
                .as_ref()
                .unwrap()
                .cash,
            Some(Decimal::new(10_000, 0))
        );
    }
    assert_eq!(
        source.calls(),
        vec![
            "input:2026-10-01",
            "price:2026-10-01:5:SPY",
            "input:2026-10-02",
            "input:2026-10-03",
            "price:2026-10-03:5:SPY",
            "input:2026-10-04",
            "price:2026-10-04:5:SPY",
        ]
    );
}

#[tokio::test]
async fn source_cannot_smuggle_future_state_into_a_historical_cell() {
    let client = Arc::new(BacktestClient::default());
    let workflow = Arc::new(WorkflowRunner::new(client.clone(), client.clone()));
    let source = Arc::new(FixtureSource {
        calls: Mutex::new(Vec::new()),
        wrong_date: true,
    });
    let runner = BacktestRunner::new(workflow, source);

    let error = runner.run(request(vec![day(1)])).await.unwrap_err();
    assert!(error.to_string().contains("as-of date"));
    assert!(client.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn benchmark_alpha_and_summary_metrics_are_explicit_and_deterministic() {
    let client = Arc::new(BacktestClient::default());
    let workflow = Arc::new(WorkflowRunner::new(client.clone(), client));
    let source = Arc::new(FixtureSource::default());
    let runner = BacktestRunner::new(workflow, source);

    let result = runner
        .run(request(vec![day(1), day(3), day(4)]))
        .await
        .unwrap();

    let first = result.cells[0].outcome.as_ref().unwrap();
    assert_eq!(first.raw_return, Decimal::new(10, 2));
    assert_eq!(first.benchmark_return, Decimal::new(5, 2));
    assert_eq!(first.alpha_return, Decimal::new(5, 2));

    let summary = result.summary();
    assert_eq!(summary.resolved, 3);
    assert_eq!(summary.pending, 0);
    assert_eq!(summary.unavailable, 0);
    let buy = summary.score(FinalRating::Buy).unwrap();
    assert_eq!(buy.count, 1);
    assert_eq!(buy.hit_rate, Some(Decimal::ONE));
    assert_eq!(buy.mean_alpha, Decimal::new(5, 2));
    let sell = summary.score(FinalRating::Sell).unwrap();
    assert_eq!(sell.hit_rate, Some(Decimal::ONE));
    let hold = summary.score(FinalRating::Hold).unwrap();
    assert_eq!(hold.hit_rate, None);
}

#[tokio::test]
async fn missing_outcome_window_is_pending_not_fabricated() {
    struct PendingSource(FixtureSource);
    #[async_trait]
    impl BacktestDataSource for PendingSource {
        async fn run_input(
            &self,
            symbol: &Symbol,
            as_of: NaiveDate,
        ) -> Result<BacktestInput, trading_agent_core::CoreError> {
            self.0.run_input(symbol, as_of).await
        }
        async fn price_window(
            &self,
            _symbol: &Symbol,
            _as_of: NaiveDate,
            _holding_days: u32,
            _benchmark: &Symbol,
        ) -> Result<Option<BacktestPriceWindow>, trading_agent_core::CoreError> {
            Ok(None)
        }
    }

    let client = Arc::new(BacktestClient::default());
    let workflow = Arc::new(WorkflowRunner::new(client.clone(), client));
    let runner = BacktestRunner::new(workflow, Arc::new(PendingSource(FixtureSource::default())));
    let result = runner.run(request(vec![day(1)])).await.unwrap();
    assert!(result.cells[0].outcome.is_none());
    assert_eq!(result.summary().pending, 1);
}
