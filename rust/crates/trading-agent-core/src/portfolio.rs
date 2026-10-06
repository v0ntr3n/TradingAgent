use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Symbol;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub symbol: Symbol,
    pub quantity: Decimal,
    pub average_price: Option<Decimal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioContext {
    pub cash: Option<Decimal>,
    pub currency: Option<String>,
    pub positions: Vec<Position>,
}

impl PortfolioContext {
    pub fn fingerprint(&self) -> String {
        let mut normalized = self.clone();
        normalized
            .positions
            .sort_by(|a, b| a.symbol.as_str().cmp(b.symbol.as_str()));
        let bytes =
            serde_json::to_vec(&normalized).expect("portfolio context is always JSON serializable");
        format!("{:x}", Sha256::digest(bytes))
    }

    pub fn render(&self, focus: &Symbol) -> String {
        let mut lines = vec![format!("Portfolio context for {focus}:")];
        match (self.cash, self.currency.as_deref()) {
            (Some(cash), Some(currency)) => lines.push(format!("- cash: {cash} {currency}")),
            (Some(cash), None) => lines.push(format!("- cash: {cash}")),
            (None, Some(currency)) => lines.push(format!("- currency: {currency}")),
            (None, None) => {}
        }

        if self.positions.is_empty() {
            lines.push("- positions: flat".into());
        } else {
            lines.push("- positions:".into());
            for position in &self.positions {
                let average = position
                    .average_price
                    .map(|price| format!(" @ average {price}"))
                    .unwrap_or_default();
                lines.push(format!(
                    "  - {}: quantity {}{}",
                    position.symbol, position.quantity, average
                ));
            }
        }
        lines.join("\n")
    }
}
