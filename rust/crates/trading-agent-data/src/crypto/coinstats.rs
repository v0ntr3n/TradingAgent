use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::common::{historical_withheld, scalar};
use crate::{DataError, DataRequest, DataStatus, FundamentalsSnapshot, HttpRequest, HttpTransport, NewsBatch};

pub struct CoinStatsClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    api_key: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl CoinStatsClient {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        now: DateTime<Utc>,
        timezone: Tz,
    ) -> Self {
        Self { transport, base_url: base_url.into(), api_key: api_key.into(), now, timezone }
    }

    fn request(&self, path: &str) -> HttpRequest {
        HttpRequest::get(&self.base_url, path)
            .with_header("accept", "application/json")
            .with_header("X-API-KEY", &self.api_key)
    }

    pub async fn btc_dominance(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<FundamentalsSnapshot>, DataError> {
        if let Some(withheld) = historical_withheld("coinstats-dominance-live", request, self.now, self.timezone) {
            return Ok(withheld);
        }
        let mut values = Vec::new();
        for (label, period) in [("24h", "24h"), ("1w", "1w")] {
            let data = self.transport.execute(
                self.request("/insights/btc-dominance").with_query("type", period),
            ).await?;
            if let Some(row) = data.get("data").and_then(|v| v.as_array()).and_then(|rows| rows.last()) {
                values.push(format!("{label}: {}%", scalar(&row[1])));
            }
        }
        if values.is_empty() {
            return Err(DataError::Vendor { vendor: "coinstats".into(), message: "no BTC dominance data returned".into() });
        }
        Ok(DataStatus::Available(FundamentalsSnapshot {
            summary: format!("# Bitcoin dominance\n{}", values.join(" | ")),
        }))
    }

    pub async fn news(
        &self,
        request: &DataRequest,
        limit: usize,
    ) -> Result<DataStatus<NewsBatch>, DataError> {
        if let Some(withheld) = historical_withheld("coinstats-news-live", request, self.now, self.timezone) {
            return Ok(withheld);
        }
        let limit = limit.clamp(1, 20);
        let data = self.transport.execute(
            self.request("/news/type/latest")
                .with_query("page", 1)
                .with_query("limit", limit),
        ).await?;
        let rows = if let Some(rows) = data.as_array() {
            rows
        } else {
            data.get("result").and_then(|v| v.as_array()).ok_or_else(|| DataError::Vendor {
                vendor: "coinstats".into(), message: "no articles returned".into()
            })?
        };
        let items = rows.iter().take(limit).map(|row| {
            format!("**{}** ({}) — {}", scalar(&row["title"]), scalar(&row["source"]), scalar(&row["description"]))
        }).collect();
        Ok(DataStatus::Available(NewsBatch { items }))
    }
}
