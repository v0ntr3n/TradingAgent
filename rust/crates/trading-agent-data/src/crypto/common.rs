use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use trading_agent_core::Symbol;

use crate::{DataAccess, DataPolicy, DataRequest, DataStatus, SourceAvailability};

pub fn crypto_base(symbol: &Symbol) -> &str {
    symbol.as_str().strip_suffix("-USD").unwrap_or(symbol.as_str())
}

pub fn binance_symbol(symbol: &Symbol) -> String {
    format!("{}USDT", crypto_base(symbol))
}

pub fn taapi_symbol(symbol: &Symbol) -> String {
    format!("{}/USDT", crypto_base(symbol))
}

pub fn historical_withheld<T>(
    source: &str,
    request: &DataRequest,
    now: DateTime<Utc>,
    timezone: Tz,
) -> Option<DataStatus<T>> {
    if DataPolicy::classify(
        request.as_of,
        SourceAvailability::CurrentOnly,
        now,
        timezone,
    ) == DataAccess::WithheldHistorical
    {
        Some(DataStatus::WithheldHistorical {
            source: source.to_owned(),
            as_of: request.as_of,
        })
    } else {
        None
    }
}

pub fn scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Null => "N/A".into(),
        other => other.to_string(),
    }
}
