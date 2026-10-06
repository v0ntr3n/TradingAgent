use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{
    AssetType, ExternalReport, InvestmentPreferences, PortfolioContext, Symbol, TraderProposal,
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentState {
    pub schema_version: u32,
    pub symbol: Symbol,
    pub asset_type: AssetType,
    pub trade_date: NaiveDate,
    pub instrument_context: Option<String>,
    pub investment_preferences: Option<InvestmentPreferences>,
    pub external_reports: Vec<ExternalReport>,
    pub portfolio: Option<PortfolioContext>,
    pub market_report: Option<String>,
    pub sentiment_report: Option<String>,
    pub news_report: Option<String>,
    pub fundamentals_report: Option<String>,
    pub research_plan: Option<String>,
    pub trader_plan: Option<String>,
    pub trader_proposal: Option<TraderProposal>,
    pub final_decision: Option<String>,
    pub past_context: Option<String>,
}

impl AgentState {
    pub fn new(symbol: Symbol, trade_date: NaiveDate) -> Self {
        let asset_type = symbol.asset_type();
        Self {
            schema_version: SCHEMA_VERSION,
            symbol,
            asset_type,
            trade_date,
            instrument_context: None,
            investment_preferences: None,
            external_reports: Vec::new(),
            portfolio: None,
            market_report: None,
            sentiment_report: None,
            news_report: None,
            fundamentals_report: None,
            research_plan: None,
            trader_plan: None,
            trader_proposal: None,
            final_decision: None,
            past_context: None,
        }
    }
}
