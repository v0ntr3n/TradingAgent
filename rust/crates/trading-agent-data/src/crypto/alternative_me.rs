use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::common::{historical_withheld, scalar};
use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, SentimentSnapshot,
    SentimentSource, SourceAvailability,
};

pub struct AlternativeMeClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl AlternativeMeClient {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        base_url: impl Into<String>,
        now: DateTime<Utc>,
        timezone: Tz,
    ) -> Self {
        Self { transport, base_url: base_url.into(), now, timezone }
    }

    pub async fn sentiment(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<SentimentSnapshot>, DataError> {
        if let Some(withheld) = historical_withheld("alternative-me-live", request, self.now, self.timezone) {
            return Ok(withheld);
        }
        let data = self.transport.execute(
            HttpRequest::get(&self.base_url, "/fng/").with_query("limit", 10),
        ).await?;
        let rows = data.get("data").and_then(|value| value.as_array()).ok_or_else(|| DataError::Vendor {
            vendor: "alternative_me".into(), message: "no data returned".into()
        })?;
        let formatted = rows.iter().map(|row| {
            format!("{} ({})", scalar(&row["value"]), scalar(&row["value_classification"]))
        }).collect::<Vec<_>>();
        Ok(DataStatus::Available(SentimentSnapshot {
            summary: format!("# Crypto Fear & Greed Index\nCurrent and previous daily readings: {}", formatted.join(", ")),
        }))
    }
}

impl DataSource for AlternativeMeClient {
    fn source_id(&self) -> &str { "alternative_me" }
    fn availability(&self) -> SourceAvailability { SourceAvailability::CurrentOnly }
}

#[async_trait]
impl SentimentSource for AlternativeMeClient {
    async fn sentiment(&self, request: &DataRequest) -> Result<DataStatus<SentimentSnapshot>, DataError> {
        AlternativeMeClient::sentiment(self, request).await
    }
}
