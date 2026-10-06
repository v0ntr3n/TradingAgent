use std::sync::Arc;

use trading_agent_llm::LlmClient;

use crate::{
    AgentState, CoreError, RunConfig,
    agents::{
        AgentContext, AgentEvidence, AgentReport, AnalystKind, ResearchSide, RiskStance,
        run_analyst, run_portfolio_manager, run_research_manager, run_researcher,
        run_risk_analyst, run_trader,
    },
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
}

impl WorkflowRunner {
    pub fn new(quick: Arc<dyn LlmClient>, deep: Arc<dyn LlmClient>) -> Self {
        Self { quick, deep }
    }

    pub async fn run(&self, input: RunInput) -> Result<RunResult, CoreError> {
        let RunInput {
            mut state,
            evidence,
            config,
        } = input;
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

        for (kind, task) in analyst_tasks {
            let report = task
                .await
                .map_err(|error| CoreError::InvalidConfig(format!("analyst task failed: {error}")))??;
            store_analyst_report(&mut state, kind, report);
        }

        let mut debate_transcript = String::new();
        for round in 1..=config.max_debate_rounds {
            let context = AgentContext {
                state: &state,
                evidence: &evidence,
                output_language: &config.output_language,
            };
            let bull = run_researcher(self.deep.as_ref(), ResearchSide::Bull, &context).await?;
            let bear = run_researcher(self.deep.as_ref(), ResearchSide::Bear, &context).await?;
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
        state.research_plan = Some(research_plan.content);

        let proposal = {
            let context = AgentContext {
                state: &state,
                evidence: &evidence,
                output_language: &config.output_language,
            };
            run_trader(self.deep.as_ref(), &context).await?
        };
        state.trader_plan = Some(proposal.reasoning.clone());
        state.trader_proposal = Some(proposal);

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
                let report = run_risk_analyst(self.deep.as_ref(), stance, &context).await?;
                risk_reports.push(report);
            }
        }

        let final_report = {
            let context = AgentContext {
                state: &state,
                evidence: &evidence,
                output_language: &config.output_language,
            };
            run_portfolio_manager(self.deep.as_ref(), &context, &risk_reports).await?
        };
        let rating = parse_final_rating(&final_report.content);
        state.final_decision = Some(final_report.content);

        Ok(RunResult { state, rating })
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
