use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::{
    CoreError, Symbol,
    workflow::{FinalRating, RunInput, RunResult, WorkflowRunner},
};

#[derive(Clone, Debug)]
pub enum BacktestInput {
    Available(Box<RunInput>),
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestPriceWindow {
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub benchmark_entry_price: Decimal,
    pub benchmark_exit_price: Decimal,
    pub resolution_date: NaiveDate,
}

#[async_trait]
pub trait BacktestDataSource: Send + Sync {
    async fn run_input(
        &self,
        symbol: &Symbol,
        as_of: NaiveDate,
    ) -> Result<BacktestInput, CoreError>;

    async fn price_window(
        &self,
        symbol: &Symbol,
        as_of: NaiveDate,
        holding_days: u32,
        benchmark: &Symbol,
    ) -> Result<Option<BacktestPriceWindow>, CoreError>;
}

#[derive(Clone, Debug)]
pub struct BacktestRequest {
    pub symbols: Vec<Symbol>,
    pub dates: Vec<NaiveDate>,
    pub holding_days: u32,
    pub benchmark: Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestOutcome {
    pub raw_return: Decimal,
    pub benchmark_return: Decimal,
    pub alpha_return: Decimal,
    pub resolution_date: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct BacktestCell {
    pub symbol: Symbol,
    pub date: NaiveDate,
    pub result: Option<RunResult>,
    pub outcome: Option<BacktestOutcome>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BacktestResult {
    pub cells: Vec<BacktestCell>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatingScore {
    pub rating: FinalRating,
    pub count: usize,
    pub hit_rate: Option<Decimal>,
    pub mean_alpha: Decimal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacktestSummary {
    pub resolved: usize,
    pub pending: usize,
    pub unavailable: usize,
    scores: Vec<RatingScore>,
}

impl BacktestSummary {
    pub fn score(&self, rating: FinalRating) -> Option<&RatingScore> {
        self.scores.iter().find(|score| score.rating == rating)
    }

    pub fn scores(&self) -> &[RatingScore] {
        &self.scores
    }
}

impl BacktestResult {
    pub fn summary(&self) -> BacktestSummary {
        let unavailable = self
            .cells
            .iter()
            .filter(|cell| cell.unavailable_reason.is_some())
            .count();
        let pending = self
            .cells
            .iter()
            .filter(|cell| cell.result.is_some() && cell.outcome.is_none())
            .count();

        let resolved_cells = self
            .cells
            .iter()
            .filter_map(|cell| Some((cell.result.as_ref()?, cell.outcome.as_ref()?)))
            .collect::<Vec<_>>();

        let mut scores = Vec::new();
        for rating in [
            FinalRating::Buy,
            FinalRating::Overweight,
            FinalRating::Hold,
            FinalRating::Underweight,
            FinalRating::Sell,
            FinalRating::Review,
        ] {
            let matching = resolved_cells
                .iter()
                .filter(|(result, _)| result.rating == rating)
                .map(|(_, outcome)| *outcome)
                .collect::<Vec<_>>();
            if matching.is_empty() {
                continue;
            }

            let sum_alpha = matching
                .iter()
                .fold(Decimal::ZERO, |acc, outcome| acc + outcome.alpha_return);
            let mean_alpha = sum_alpha / Decimal::from(matching.len() as u64);
            let direction = rating_direction(rating);
            let hit_rate = if direction == Decimal::ZERO {
                None
            } else {
                let hits = matching
                    .iter()
                    .filter(|outcome| outcome.alpha_return * direction > Decimal::ZERO)
                    .count();
                Some(Decimal::from(hits as u64) / Decimal::from(matching.len() as u64))
            };

            scores.push(RatingScore {
                rating,
                count: matching.len(),
                hit_rate,
                mean_alpha,
            });
        }

        BacktestSummary {
            resolved: resolved_cells.len(),
            pending,
            unavailable,
            scores,
        }
    }
}

pub struct BacktestRunner {
    workflow: Arc<WorkflowRunner>,
    source: Arc<dyn BacktestDataSource>,
}

impl BacktestRunner {
    pub fn new(workflow: Arc<WorkflowRunner>, source: Arc<dyn BacktestDataSource>) -> Self {
        Self { workflow, source }
    }

    pub async fn run(&self, request: BacktestRequest) -> Result<BacktestResult, CoreError> {
        if request.symbols.is_empty() {
            return Err(CoreError::InvalidConfig(
                "backtest requires at least one symbol".into(),
            ));
        }
        if request.dates.is_empty() {
            return Err(CoreError::InvalidConfig(
                "backtest requires at least one analysis date".into(),
            ));
        }
        if request.holding_days == 0 {
            return Err(CoreError::InvalidConfig(
                "backtest holding_days must be at least 1".into(),
            ));
        }

        let mut symbols = request.symbols;
        symbols.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        symbols.dedup_by(|left, right| left.as_str() == right.as_str());

        let dates = request.dates.into_iter().collect::<BTreeSet<_>>();

        let mut cells = Vec::with_capacity(symbols.len() * dates.len());
        for symbol in symbols {
            for date in &dates {
                let as_of = *date;
                match self.source.run_input(&symbol, as_of).await? {
                    BacktestInput::Unavailable { reason } => {
                        cells.push(BacktestCell {
                            symbol: symbol.clone(),
                            date: as_of,
                            result: None,
                            outcome: None,
                            unavailable_reason: Some(reason),
                        });
                    }
                    BacktestInput::Available(input) => {
                        validate_input(&symbol, as_of, &input)?;
                        let result = self.workflow.run(*input).await?;
                        let outcome = self
                            .source
                            .price_window(&symbol, as_of, request.holding_days, &request.benchmark)
                            .await?
                            .map(compute_outcome)
                            .transpose()?;
                        cells.push(BacktestCell {
                            symbol: symbol.clone(),
                            date: as_of,
                            result: Some(result),
                            outcome,
                            unavailable_reason: None,
                        });
                    }
                }
            }
        }

        Ok(BacktestResult { cells })
    }
}

fn validate_input(symbol: &Symbol, as_of: NaiveDate, input: &RunInput) -> Result<(), CoreError> {
    if input.state.trade_date != as_of {
        return Err(CoreError::InvalidConfig(format!(
            "backtest source returned state for {}, expected as-of date {as_of}",
            input.state.trade_date
        )));
    }
    if input.state.symbol != *symbol {
        return Err(CoreError::InvalidConfig(format!(
            "backtest source returned symbol {}, expected {symbol}",
            input.state.symbol
        )));
    }
    Ok(())
}

fn compute_outcome(window: BacktestPriceWindow) -> Result<BacktestOutcome, CoreError> {
    if window.entry_price <= Decimal::ZERO || window.benchmark_entry_price <= Decimal::ZERO {
        return Err(CoreError::InvalidConfig(
            "backtest entry prices must be positive".into(),
        ));
    }
    let raw_return = (window.exit_price - window.entry_price) / window.entry_price;
    let benchmark_return =
        (window.benchmark_exit_price - window.benchmark_entry_price) / window.benchmark_entry_price;
    Ok(BacktestOutcome {
        raw_return,
        benchmark_return,
        alpha_return: raw_return - benchmark_return,
        resolution_date: window.resolution_date,
    })
}

fn rating_direction(rating: FinalRating) -> Decimal {
    match rating {
        FinalRating::Buy | FinalRating::Overweight => Decimal::ONE,
        FinalRating::Underweight | FinalRating::Sell => -Decimal::ONE,
        FinalRating::Hold | FinalRating::Review => Decimal::ZERO,
    }
}
