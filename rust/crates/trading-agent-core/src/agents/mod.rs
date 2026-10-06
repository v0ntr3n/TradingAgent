use chrono::NaiveDate;

use crate::{AgentState, CoreError};

mod analysts;
mod managers;
mod researchers;
mod risk;
mod trader;

pub use analysts::run_analyst;
pub use managers::{run_portfolio_manager, run_research_manager};
pub use researchers::run_researcher;
pub use risk::run_risk_analyst;
pub use trader::run_trader;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evidence {
    Available(String),
    Unavailable { source: String, reason: String },
    WithheldHistorical { source: String, as_of: NaiveDate },
}

impl Evidence {
    pub fn render(&self) -> String {
        match self {
            Self::Available(content) => format!("EVIDENCE STATUS: AVAILABLE\n{content}"),
            Self::Unavailable { source, reason } => {
                format!("EVIDENCE STATUS: UNAVAILABLE\nsource: {source}\nreason: {reason}")
            }
            Self::WithheldHistorical { source, as_of } => format!(
                "EVIDENCE STATUS: WITHHELD_HISTORICAL\nsource: {source}\nas_of: {as_of}"
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEvidence {
    pub market: Evidence,
    pub sentiment: Evidence,
    pub news: Evidence,
    pub fundamentals: Evidence,
}

pub struct AgentContext<'a> {
    pub state: &'a AgentState,
    pub evidence: &'a AgentEvidence,
    pub output_language: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalystKind {
    Market,
    Sentiment,
    News,
    Fundamentals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchSide {
    Bull,
    Bear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskStance {
    Aggressive,
    Neutral,
    Conservative,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReport {
    pub content: String,
}

pub(crate) fn llm_error(error: trading_agent_llm::LlmError) -> CoreError {
    CoreError::Llm(error.to_string())
}

pub(crate) fn common_context(context: &AgentContext<'_>, include_portfolio: bool) -> String {
    let state = context.state;
    let mut sections = vec![
        format!("Symbol: {}", state.symbol),
        format!("Asset type: {:?}", state.asset_type),
        format!("Trade date: {}", state.trade_date),
        format!("Output language: {}", context.output_language),
    ];

    if let Some(instrument) = &state.instrument_context {
        sections.push(format!("Instrument context:\n{instrument}"));
    }
    if let Some(preferences) = &state.investment_preferences {
        sections.push(preferences.render_for_prompt());
    }
    for report in &state.external_reports {
        sections.push(report.render_untrusted());
    }
    if include_portfolio {
        if let Some(portfolio) = &state.portfolio {
            sections.push(portfolio.render(&state.symbol));
        } else {
            sections.push("Portfolio context: not provided".into());
        }
    }
    if let Some(value) = &state.market_report {
        sections.push(format!("Market report:\n{value}"));
    }
    if let Some(value) = &state.sentiment_report {
        sections.push(format!("Sentiment report:\n{value}"));
    }
    if let Some(value) = &state.news_report {
        sections.push(format!("News report:\n{value}"));
    }
    if let Some(value) = &state.fundamentals_report {
        sections.push(format!("Fundamentals report:\n{value}"));
    }
    if let Some(value) = &state.research_plan {
        sections.push(format!("Research plan:\n{value}"));
    }
    if let Some(value) = &state.trader_proposal {
        if let Ok(json) = serde_json::to_string_pretty(value) {
            sections.push(format!("Trader proposal:\n{json}"));
        }
    }
    if let Some(value) = &state.past_context {
        sections.push(format!("Past decision context:\n{value}"));
    }
    sections.join("\n\n")
}

pub(crate) fn evidence_for(kind: AnalystKind, evidence: &AgentEvidence) -> &Evidence {
    match kind {
        AnalystKind::Market => &evidence.market,
        AnalystKind::Sentiment => &evidence.sentiment,
        AnalystKind::News => &evidence.news,
        AnalystKind::Fundamentals => &evidence.fundamentals,
    }
}
