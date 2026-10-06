use std::sync::Arc;

use trading_agent_llm::LlmClient;

use crate::{
    AgentState, CoreError, RunConfig, SCHEMA_VERSION,
    agents::{
        AgentContext, AgentEvidence, AgentReport, AnalystKind, ResearchSide, RiskStance,
        run_analyst, run_portfolio_manager, run_research_manager, run_researcher,
        run_risk_analyst, run_trader,
    },
    checkpoint::{CheckpointEnvelope, CheckpointStore, CompletedStage, RunSignature},
    events::{EventSink, RunEvent, WorkflowStage},
    memory::{DecisionMemory, DecisionRecord, SettlementHook},
};

use super::{
    FinalRating,
    debate::{append_debate_round, parse_final_rating},
};

#[derive(Clone, Debug)]
pub struct RunInput {
    pub state: AgentState,
    pub evidence: AgentEvidence,
    pub config: RunConfig,
}

#[derive(Clone, Debug)]
pub struct RunResult {
    pub state: AgentState,
    pub rating: FinalRating,
}

pub struct WorkflowRunner {
    quick: Arc<dyn LlmClient>,
    deep: Arc<dyn LlmClient>,
    checkpoint_store: Option<Arc<dyn CheckpointStore>>,
    event_sink: Option<Arc<dyn EventSink>>,
    decision_memory: Option<Arc<dyn DecisionMemory>>,
    settlement_hook: Option<Arc<dyn SettlementHook>>,
}

impl WorkflowRunner {
    pub fn new(quick: Arc<dyn LlmClient>, deep: Arc<dyn LlmClient>) -> Self {
        Self {
            quick,
            deep,
            checkpoint_store: None,
            event_sink: None,
            decision_memory: None,
            settlement_hook: None,
        }
    }

    pub fn with_checkpoint_store(mut self, store: Arc<dyn CheckpointStore>) -> Self {
        self.checkpoint_store = Some(store);
        self
    }

    pub fn with_event_sink(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    pub fn with_decision_memory(mut self, memory: Arc<dyn DecisionMemory>) -> Self {
        self.decision_memory = Some(memory);
        self
    }

    pub fn with_settlement_hook(mut self, hook: Arc<dyn SettlementHook>) -> Self {
        self.settlement_hook = Some(hook);
        self
    }

    pub async fn run(&self, mut input: RunInput) -> Result<RunResult, CoreError> {
        if let Some(memory) = &self.decision_memory {
            if let Some(hook) = &self.settlement_hook {
                let _ = hook.settle_pending(memory.as_ref()).await;
            }
            input.state.past_context = memory.context_as_of(
                &input.state.symbol.to_string(),
                Some(input.state.trade_date),
            )?.into();
            if input.state.past_context.as_deref() == Some("") {
                input.state.past_context = None;
            }
        }

        let signature = RunSignature::from_inputs(&input);
        let metadata = RunSignature::safe_metadata(&input);
        let mut state = input.state.clone();
        let evidence = input.evidence.clone();
        let config = input.config.clone();
        let mut completed = None;

        if let Some(store) = &self.checkpoint_store {
            if let Some(envelope) = store.load()? {
                if envelope.schema_version != SCHEMA_VERSION {
                    return Err(CoreError::Checkpoint(format!(
                        "checkpoint schema version {} is incompatible with {}",
                        envelope.schema_version, SCHEMA_VERSION
                    )));
                }
                if envelope.signature == signature {
                    state = envelope.state;
                    completed = Some(envelope.completed_stage);
                } else {
                    store.clear()?;
                }
            }
        }

        self.emit(&RunEvent::Started {
            signature: signature.clone(),
            metadata,
        })?;

        if completed < Some(CompletedStage::Analysts) {
            let selected = parse_analysts(&config.analysts)?;
            let mut analyst_tasks = Vec::with_capacity(selected.len());
            for kind in selected {
                let client = self.quick.clone();
                let state_snapshot = state.clone();
                let evidence_snapshot = evidence.clone();
                let output_language = config.output_language.clone();
                let task = tokio::spawn(async move {
                    let context = AgentContext {
                        state: &state_snapshot,
                        evidence: &evidence_snapshot,
                        output_language: &output_language,
                    };
                    run_analyst(client.as_ref(), kind, &context).await
                });
                analyst_tasks.push((kind, task));
            }

            let mut analyst_sections = Vec::new();
            for (kind, task) in analyst_tasks {
                let report = task.await.map_err(|error| {
                    CoreError::InvalidConfig(format!("analyst task failed: {error}"))
                })??;
                analyst_sections.push(report.content.clone());
                store_analyst_report(&mut state, kind, report);
            }
            self.save_checkpoint(&signature, CompletedStage::Analysts, &state)?;
            self.emit(&RunEvent::SectionCompleted {
                stage: WorkflowStage::Analysts,
                section: "analysts".into(),
                content: analyst_sections.join("\n\n"),
            })?;
        }

        if completed < Some(CompletedStage::Research) {
            let mut debate_transcript = String::new();
            for round in 1..=config.max_debate_rounds {
                let context = AgentContext {
                    state: &state,
                    evidence: &evidence,
                    output_language: &config.output_language,
                };
                let bull = run_researcher(self.quick.as_ref(), ResearchSide::Bull, &context).await?;
                let bear = run_researcher(self.quick.as_ref(), ResearchSide::Bear, &context).await?;
                append_debate_round(&mut debate_transcript, round, &bull, &bear);
            }

            let research_plan = {
                let context = AgentContext {
                    state: &state,
                    evidence: &evidence,
                    output_language: &config.output_language,
                };
                run_research_manager(self.deep.as_ref(), &context, &debate_transcript).await?
            };
            state.research_plan = Some(research_plan.content.clone());
            self.save_checkpoint(&signature, CompletedStage::Research, &state)?;
            self.emit(&RunEvent::SectionCompleted {
                stage: WorkflowStage::Research,
                section: "research".into(),
                content: research_plan.content,
            })?;
        }

        if completed < Some(CompletedStage::Trader) {
            let proposal = {
                let context = AgentContext {
                    state: &state,
                    evidence: &evidence,
                    output_language: &config.output_language,
                };
                run_trader(self.quick.as_ref(), &context).await?
            };
            state.trader_plan = Some(proposal.reasoning.clone());
            state.trader_proposal = Some(proposal);
            self.save_checkpoint(&signature, CompletedStage::Trader, &state)?;
            self.emit(&RunEvent::SectionCompleted {
                stage: WorkflowStage::Trader,
                section: "trader".into(),
                content: state.trader_plan.clone().unwrap_or_default(),
            })?;
        }

        let mut risk_reports = Vec::new();
        for _ in 0..config.max_risk_rounds {
            for stance in [
                RiskStance::Aggressive,
                RiskStance::Neutral,
                RiskStance::Conservative,
            ] {
                let context = AgentContext {
                    state: &state,
                    evidence: &evidence,
                    output_language: &config.output_language,
                };
                let report = run_risk_analyst(self.quick.as_ref(), stance, &context).await?;
                risk_reports.push(report);
            }
        }
        self.emit(&RunEvent::SectionCompleted {
            stage: WorkflowStage::Risk,
            section: "risk".into(),
            content: risk_reports
                .iter()
                .map(|report| report.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        })?;

        let final_report = {
            let context = AgentContext {
                state: &state,
                evidence: &evidence,
                output_language: &config.output_language,
            };
            run_portfolio_manager(self.deep.as_ref(), &context, &risk_reports).await?
        };
        let rating = parse_final_rating(&final_report.content);
        state.final_decision = Some(final_report.content.clone());
        self.emit(&RunEvent::SectionCompleted {
            stage: WorkflowStage::Portfolio,
            section: "portfolio".into(),
            content: final_report.content,
        })?;

        if let Some(memory) = &self.decision_memory {
            memory.store_decision(DecisionRecord {
                ticker: state.symbol.to_string(),
                trade_date: state.trade_date,
                final_decision: state.final_decision.clone().unwrap_or_default(),
                rating,
                resolved: None,
            })?;
        }
        if let Some(store) = &self.checkpoint_store {
            store.clear()?;
        }
        self.emit(&RunEvent::Completed { rating })?;

        Ok(RunResult { state, rating })
    }

    fn emit(&self, event: &RunEvent) -> Result<(), CoreError> {
        if let Some(sink) = &self.event_sink {
            sink.emit(event)?;
        }
        Ok(())
    }

    fn save_checkpoint(
        &self,
        signature: &RunSignature,
        completed_stage: CompletedStage,
        state: &AgentState,
    ) -> Result<(), CoreError> {
        if let Some(store) = &self.checkpoint_store {
            store.save(&CheckpointEnvelope {
                schema_version: SCHEMA_VERSION,
                signature: signature.clone(),
                completed_stage,
                state: state.clone(),
            })?;
        }
        Ok(())
    }
}

fn parse_analysts(values: &[String]) -> Result<Vec<AnalystKind>, CoreError> {
    let mut parsed = Vec::new();
    for value in values {
        let kind = match value.trim().to_ascii_lowercase().as_str() {
            "market" => AnalystKind::Market,
            "sentiment" => AnalystKind::Sentiment,
            "news" => AnalystKind::News,
            "fundamentals" => AnalystKind::Fundamentals,
            other => {
                return Err(CoreError::InvalidConfig(format!(
                    "unsupported analyst: {other}"
                )));
            }
        };
        if !parsed.contains(&kind) {
            parsed.push(kind);
        }
    }
    Ok(parsed)
}

fn store_analyst_report(state: &mut AgentState, kind: AnalystKind, report: AgentReport) {
    match kind {
        AnalystKind::Market => state.market_report = Some(report.content),
        AnalystKind::Sentiment => state.sentiment_report = Some(report.content),
        AnalystKind::News => state.news_report = Some(report.content),
        AnalystKind::Fundamentals => state.fundamentals_report = Some(report.content),
    }
}
