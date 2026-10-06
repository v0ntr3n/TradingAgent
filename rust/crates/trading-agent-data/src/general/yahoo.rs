use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Days, Utc};

use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, MarketDataSource,
    MarketSnapshot, SourceAvailability,
};

pub struct YahooProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
}

impl YahooProvider {
    pub fn new(http: Arc<dyn HttpTransport>, base_url: impl Into<String>) -> Self {
        Self { http, base_url: base_url.into() }
    }
}

impl DataSource for YahooProvider {
    fn source_id(&self) -> &str { "yahoo" }
    fn availability(&self) -> SourceAvailability { SourceAvailability::Archival }
}

#[async_trait]
impl MarketDataSource for YahooProvider {
    async fn market_snapshot(&self, request: &DataRequest) -> Result<DataStatus<MarketSnapshot>, DataError> {
        let response = self.http.execute(
            HttpRequest::get(&self.base_url, format!("/v8/finance/chart/{}", request.symbol.as_str()))
                .with_query("interval", "1d")
                .with_query("range", "3mo"),
        ).await?;
        let result = response.pointer("/chart/result/0").ok_or_else(|| DataError::Vendor {
            vendor: "yahoo".into(), message: "missing chart result".into(),
        })?;
        let timestamps = result.get("timestamp").and_then(|v| v.as_array()).ok_or_else(|| DataError::Vendor {
            vendor: "yahoo".into(), message: "missing timestamps".into(),
        })?;
        let last_ts = timestamps.last().and_then(|v| v.as_i64()).ok_or_else(|| DataError::Vendor {
            vendor: "yahoo".into(), message: "missing latest timestamp".into(),
        })?;
        let last_date = DateTime::<Utc>::from_timestamp(last_ts, 0).ok_or_else(|| DataError::Vendor {
            vendor: "yahoo".into(), message: "invalid timestamp".into(),
        })?.date_naive();
        let stale_cutoff = request.as_of.checked_sub_days(Days::new(7)).unwrap_or(request.as_of);
        if last_date < stale_cutoff {
            return Ok(DataStatus::Unavailable {
                source: self.source_id().into(),
                reason: format!("stale market data: latest {last_date}, requested {}", request.as_of),
            });
        }
        let close = result.pointer("/indicators/quote/0/close").and_then(|v| v.as_array()).and_then(|a| a.last()).and_then(|v| v.as_f64());
        Ok(DataStatus::Available(MarketSnapshot {
            summary: format!("Yahoo daily market data as of {last_date}; close={}", close.map(|v| v.to_string()).unwrap_or_else(|| "n/a".into())),
        }))
    }
}
