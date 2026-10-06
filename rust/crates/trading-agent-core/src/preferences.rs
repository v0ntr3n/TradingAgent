use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenuePreference {
    Spot,
    Futures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStyle {
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestmentPreferences {
    pub venue: Option<VenuePreference>,
    #[serde(default)]
    pub allow_long: bool,
    #[serde(default)]
    pub allow_short: bool,
    pub style: Option<RiskStyle>,
    pub holding_horizon: Option<String>,
    pub min_leverage: Option<Decimal>,
    pub max_leverage: Option<Decimal>,
    pub max_loss_pct: Option<Decimal>,
    pub free_form: Option<String>,
}

impl InvestmentPreferences {
    pub fn from_json_str(input: &str) -> Result<Self, CoreError> {
        let preferences: Self = serde_json::from_str(input)?;
        preferences.validate()?;
        Ok(preferences)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if let Some(value) = self.min_leverage {
            if value <= Decimal::ZERO {
                return Err(CoreError::InvalidPreferences(
                    "minimum leverage must be positive".into(),
                ));
            }
        }
        if let Some(value) = self.max_leverage {
            if value <= Decimal::ZERO {
                return Err(CoreError::InvalidPreferences(
                    "maximum leverage must be positive".into(),
                ));
            }
        }
        if let (Some(min), Some(max)) = (self.min_leverage, self.max_leverage) {
            if min > max {
                return Err(CoreError::InvalidPreferences(
                    "minimum leverage cannot exceed maximum leverage".into(),
                ));
            }
        }
        if let Some(loss) = self.max_loss_pct {
            if loss < Decimal::ZERO || loss > Decimal::ONE_HUNDRED {
                return Err(CoreError::InvalidPreferences(
                    "maximum loss percentage must be between 0 and 100".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn render_for_prompt(&self) -> String {
        let mut lines = vec!["Investment preferences (user-provided constraints):".to_string()];
        if let Some(venue) = self.venue {
            lines.push(format!(
                "- venue: {}",
                match venue {
                    VenuePreference::Spot => "spot",
                    VenuePreference::Futures => "futures",
                }
            ));
        }
        lines.push(format!("- long positions allowed: {}", self.allow_long));
        lines.push(format!("- short positions allowed: {}", self.allow_short));
        if let Some(style) = self.style {
            lines.push(format!(
                "- risk style: {}",
                match style {
                    RiskStyle::Conservative => "conservative",
                    RiskStyle::Balanced => "balanced",
                    RiskStyle::Aggressive => "aggressive",
                }
            ));
        }
        if let Some(horizon) = &self.holding_horizon {
            lines.push(format!("- holding horizon: {horizon}"));
        }
        if let Some(value) = self.min_leverage {
            lines.push(format!("- minimum leverage: {value}x"));
        }
        if let Some(value) = self.max_leverage {
            lines.push(format!("- maximum leverage: {value}x"));
        }
        if let Some(value) = self.max_loss_pct {
            lines.push(format!("- maximum acceptable loss: {value}%"));
        }
        if let Some(extra) = &self.free_form {
            lines.push(format!("- additional instructions: {extra}"));
        }
        lines.join("\n")
    }
}
