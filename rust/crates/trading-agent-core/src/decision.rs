use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraderAction {
    Buy,
    Hold,
    Sell,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionSizing {
    pub description: String,
    pub percent_of_portfolio: Option<Decimal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraderProposal {
    pub action: TraderAction,
    pub reasoning: String,
    pub entry_price: Option<Decimal>,
    pub support: Option<Decimal>,
    pub resistance: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub position_sizing: Option<PositionSizing>,
}

#[derive(Deserialize)]
struct RawPositionSizing {
    description: String,
    percent_of_portfolio: Option<Value>,
}

#[derive(Deserialize)]
struct RawTraderProposal {
    action: TraderAction,
    reasoning: String,
    entry_price: Option<Value>,
    support: Option<Value>,
    resistance: Option<Value>,
    take_profit: Option<Value>,
    stop_loss: Option<Value>,
    position_sizing: Option<RawPositionSizing>,
}

impl TraderProposal {
    pub fn from_json_str(input: &str) -> Result<Self, CoreError> {
        let raw: RawTraderProposal = serde_json::from_str(input)?;
        let position_sizing = match raw.position_sizing {
            Some(sizing) => Some(PositionSizing {
                description: sizing.description,
                percent_of_portfolio: parse_optional_decimal(
                    sizing.percent_of_portfolio,
                    "position_sizing.percent_of_portfolio",
                )?,
            }),
            None => None,
        };

        Ok(Self {
            action: raw.action,
            reasoning: raw.reasoning,
            entry_price: parse_optional_decimal(raw.entry_price, "entry_price")?,
            support: parse_optional_decimal(raw.support, "support")?,
            resistance: parse_optional_decimal(raw.resistance, "resistance")?,
            take_profit: parse_optional_decimal(raw.take_profit, "take_profit")?,
            stop_loss: parse_optional_decimal(raw.stop_loss, "stop_loss")?,
            position_sizing,
        })
    }

    pub fn validate(self) -> Result<Self, CoreError> {
        for (name, value) in [
            ("entry_price", self.entry_price),
            ("support", self.support),
            ("resistance", self.resistance),
            ("take_profit", self.take_profit),
            ("stop_loss", self.stop_loss),
        ] {
            if value.is_some_and(|price| price <= Decimal::ZERO) {
                return Err(CoreError::InvalidDecision(format!(
                    "{name} must be an absolute positive price"
                )));
            }
        }

        if let (Some(support), Some(resistance)) = (self.support, self.resistance) {
            if support > resistance {
                return Err(CoreError::InvalidDecision(
                    "support cannot exceed resistance".into(),
                ));
            }
        }

        if let Some(sizing) = &self.position_sizing {
            if sizing
                .percent_of_portfolio
                .is_some_and(|pct| pct <= Decimal::ZERO || pct > Decimal::ONE_HUNDRED)
            {
                return Err(CoreError::InvalidDecision(
                    "position percentage must be greater than 0 and at most 100".into(),
                ));
            }
        }

        if let Some(entry) = self.entry_price {
            match self.action {
                TraderAction::Buy => {
                    if self.stop_loss.is_some_and(|stop| stop >= entry) {
                        return Err(CoreError::InvalidDecision(
                            "buy stop-loss must be below entry".into(),
                        ));
                    }
                    if self.take_profit.is_some_and(|target| target <= entry) {
                        return Err(CoreError::InvalidDecision(
                            "buy take-profit must be above entry".into(),
                        ));
                    }
                }
                TraderAction::Sell => {
                    if self.stop_loss.is_some_and(|stop| stop <= entry) {
                        return Err(CoreError::InvalidDecision(
                            "sell stop-loss must be above entry".into(),
                        ));
                    }
                    if self.take_profit.is_some_and(|target| target >= entry) {
                        return Err(CoreError::InvalidDecision(
                            "sell take-profit must be below entry".into(),
                        ));
                    }
                }
                TraderAction::Hold => {}
            }
        }

        Ok(self)
    }
}

fn parse_optional_decimal(value: Option<Value>, field: &str) -> Result<Option<Decimal>, CoreError> {
    value.map(|value| parse_decimal(value, field)).transpose()
}

fn parse_decimal(value: Value, field: &str) -> Result<Decimal, CoreError> {
    let text = match value {
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        other => {
            return Err(CoreError::InvalidDecision(format!(
                "{field} must be a decimal string or JSON number, got {other}"
            )));
        }
    };

    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.contains('%')
        || trimmed.contains("..")
        || trimmed.eq_ignore_ascii_case("nan")
        || trimmed.to_ascii_lowercase().contains("inf")
    {
        return Err(CoreError::InvalidDecision(format!(
            "{field} must be an absolute decimal value"
        )));
    }

    Decimal::from_str(trimmed).map_err(|_| {
        CoreError::InvalidDecision(format!("{field} must be an absolute decimal value"))
    })
}
