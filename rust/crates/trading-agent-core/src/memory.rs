use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{CoreError, workflow::FinalRating};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettlementOutcome {
    pub raw_return: f64,
    pub alpha_return: f64,
    pub holding_days: u32,
    pub resolution_date: NaiveDate,
    pub reflection: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub ticker: String,
    pub trade_date: NaiveDate,
    pub final_decision: String,
    pub rating: FinalRating,
    pub resolved: Option<SettlementOutcome>,
}

pub trait DecisionMemory: Send + Sync {
    fn store_decision(&self, record: DecisionRecord) -> Result<(), CoreError>;
    fn pending(&self) -> Result<Vec<DecisionRecord>, CoreError>;
    fn settle(
        &self,
        ticker: &str,
        trade_date: NaiveDate,
        outcome: SettlementOutcome,
    ) -> Result<bool, CoreError>;
    fn context_as_of(&self, ticker: &str, as_of: Option<NaiveDate>) -> Result<String, CoreError>;
}

#[async_trait]
pub trait SettlementHook: Send + Sync {
    async fn settle_pending(&self, memory: &dyn DecisionMemory) -> Result<(), CoreError>;
}

#[derive(Debug)]
pub struct JsonDecisionMemory {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonDecisionMemory {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            lock: Mutex::new(()),
        }
    }

    fn load_records(&self) -> Result<Vec<DecisionRecord>, CoreError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| CoreError::Persistence(format!("read decision memory: {error}")))?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&bytes).map_err(CoreError::from)
    }

    fn write_records(&self, records: &[DecisionRecord]) -> Result<(), CoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CoreError::Persistence(format!("create memory directory: {error}"))
            })?;
        }
        let temp = self.path.with_extension("tmp");
        let bytes = serde_json::to_vec_pretty(records)?;
        fs::write(&temp, bytes)
            .map_err(|error| CoreError::Persistence(format!("write decision memory: {error}")))?;
        fs::rename(&temp, &self.path)
            .map_err(|error| CoreError::Persistence(format!("replace decision memory: {error}")))?;
        Ok(())
    }
}

impl DecisionMemory for JsonDecisionMemory {
    fn store_decision(&self, record: DecisionRecord) -> Result<(), CoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| CoreError::Persistence("decision memory lock poisoned".into()))?;
        let mut records = self.load_records()?;
        if records.iter().any(|existing| {
            existing.ticker == record.ticker && existing.trade_date == record.trade_date
        }) {
            return Ok(());
        }
        records.push(record);
        self.write_records(&records)
    }

    fn pending(&self) -> Result<Vec<DecisionRecord>, CoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| CoreError::Persistence("decision memory lock poisoned".into()))?;
        Ok(self
            .load_records()?
            .into_iter()
            .filter(|record| record.resolved.is_none())
            .collect())
    }

    fn settle(
        &self,
        ticker: &str,
        trade_date: NaiveDate,
        outcome: SettlementOutcome,
    ) -> Result<bool, CoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| CoreError::Persistence("decision memory lock poisoned".into()))?;
        let mut records = self.load_records()?;
        let Some(record) = records.iter_mut().find(|record| {
            record.ticker == ticker && record.trade_date == trade_date && record.resolved.is_none()
        }) else {
            return Ok(false);
        };
        record.resolved = Some(outcome);
        self.write_records(&records)?;
        Ok(true)
    }

    fn context_as_of(&self, ticker: &str, as_of: Option<NaiveDate>) -> Result<String, CoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| CoreError::Persistence("decision memory lock poisoned".into()))?;
        let mut resolved = self
            .load_records()?
            .into_iter()
            .filter(|record| {
                record.resolved.as_ref().is_some_and(|outcome| {
                    as_of.is_none_or(|cutoff| outcome.resolution_date <= cutoff)
                })
            })
            .collect::<Vec<_>>();
        resolved.sort_by_key(|record| record.trade_date);
        resolved.reverse();

        let same = resolved
            .iter()
            .filter(|record| record.ticker == ticker)
            .take(5)
            .collect::<Vec<_>>();
        let cross = resolved
            .iter()
            .filter(|record| record.ticker != ticker)
            .take(3)
            .collect::<Vec<_>>();

        let mut sections = Vec::new();
        if !same.is_empty() {
            sections.push(format!("Past analyses of {ticker} (most recent first):"));
            for record in same {
                if let Some(outcome) = &record.resolved {
                    sections.push(format!(
                        "[{} | {} | {:?} | raw {:+.1}% | alpha {:+.1}% | {}d]\nDECISION:\n{}\n\nREFLECTION:\n{}",
                        record.trade_date,
                        record.ticker,
                        record.rating,
                        outcome.raw_return * 100.0,
                        outcome.alpha_return * 100.0,
                        outcome.holding_days,
                        record.final_decision,
                        outcome.reflection,
                    ));
                }
            }
        }
        if !cross.is_empty() {
            sections.push("Recent cross-ticker lessons:".into());
            for record in cross {
                if let Some(outcome) = &record.resolved {
                    sections.push(format!(
                        "[{} | {} | {:?} | raw {:+.1}%]\n{}",
                        record.trade_date,
                        record.ticker,
                        record.rating,
                        outcome.raw_return * 100.0,
                        outcome.reflection,
                    ));
                }
            }
        }
        Ok(sections.join("\n\n"))
    }
}
