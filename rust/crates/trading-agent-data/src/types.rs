use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use trading_agent_core::Symbol;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceAvailability {
    Archival,
    CurrentOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataAccess {
    Allowed,
    WithheldHistorical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataStatus<T> {
    Available(T),
    Unavailable { source: String, reason: String },
    WithheldHistorical { source: String, as_of: NaiveDate },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataRequest {
    pub symbol: Symbol,
    pub as_of: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechnicalIndicators {
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsBatch {
    pub items: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentimentSnapshot {
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundamentalsSnapshot {
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroSnapshot {
    pub summary: String,
}
