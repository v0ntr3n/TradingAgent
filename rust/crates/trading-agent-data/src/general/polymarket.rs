use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, SentimentSnapshot,
    SentimentSource, SourceAvailability,
};

pub struct PolymarketProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
    current_date: NaiveDate,
}

impl PolymarketProvider {
    pub fn new(http: Arc<dyn HttpTransport>, base_url: impl Into<String>, current_date: NaiveDate) -> Self {
        Self { http, base_url: base_url.into(), current_date }
    }
}

impl DataSource for PolymarketProvider {
    fn source_id(&self) -> &str { "polymarket" }
    fn availability(&self) -> SourceAvailability { SourceAvailability::CurrentOnly }
}

#[async_trait]
impl SentimentSource for PolymarketProvider {
    async fn sentiment(&self, request: &DataRequest) -> Result<DataStatus<SentimentSnapshot>, DataError> {
        if request.as_of < self.current_date {
            return Ok(DataStatus::WithheldHistorical { source: self.source_id().into(), as_of: request.as_of });
        }
        let response = self.http.execute(
            HttpRequest::get(&self.base_url, "/markets")
                .with_query("active", "true")
                .with_query("search", request.symbol.as_str()),
        ).await?;
        let count = response.get("markets").and_then(|v| v.as_array()).map(|v| v.len()).unwrap_or(0);
        Ok(DataStatus::Available(SentimentSnapshot { summary: format!("Polymarket active markets matching {}: {count}", request.symbol.as_str()) }))
    }
}
