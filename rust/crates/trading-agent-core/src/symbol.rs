use std::fmt;

use serde::{Deserialize, Serialize};

use crate::CoreError;

const CRYPTO_BASES: &[&str] = &[
    "BTC", "ETH", "BNB", "SOL", "XRP", "ADA", "DOGE", "AVAX", "DOT", "LINK", "LTC",
    "BCH", "TRX", "TON", "SHIB", "APT", "ARB", "OP", "SUI",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Stock,
    Crypto,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    canonical: String,
    asset_type: AssetType,
}

impl Symbol {
    pub fn parse(raw: &str) -> Result<Self, CoreError> {
        let value = raw.trim().to_ascii_uppercase();
        if value.is_empty()
            || value.len() > 32
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        {
            return Err(CoreError::InvalidSymbol(raw.to_owned()));
        }

        if let Some(base) = crypto_base(&value) {
            return Ok(Self {
                canonical: format!("{base}-USD"),
                asset_type: AssetType::Crypto,
            });
        }

        if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
            return Err(CoreError::InvalidSymbol(raw.to_owned()));
        }

        Ok(Self {
            canonical: value,
            asset_type: AssetType::Stock,
        })
    }

    pub fn asset_type(&self) -> AssetType {
        self.asset_type
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn crypto_base(value: &str) -> Option<&str> {
    if CRYPTO_BASES.contains(&value) {
        return Some(value);
    }

    if let Some((base, quote)) = value.split_once('-') {
        if CRYPTO_BASES.contains(&base) && matches!(quote, "USD" | "USDT" | "USDC") {
            return Some(base);
        }
    }

    for quote in ["USDT", "USDC", "USD"] {
        if let Some(base) = value.strip_suffix(quote) {
            if CRYPTO_BASES.contains(&base) {
                return Some(base);
            }
        }
    }

    None
}
