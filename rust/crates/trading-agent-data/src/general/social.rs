use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, SentimentSnapshot,
    SentimentSource, SourceAvailability,
};

pub struct SocialProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
    current_date: NaiveDate,
}

impl SocialProvider {
    pub fn new(http: Arc<dyn HttpTransport>, base_url: impl Into<String>, current_date: NaiveDate) -> Self {
        Self { http, base_url: base_url.into(), current_date }
    }
}

impl DataSource for SocialProvider {
    fn source_id(&self) -> &str { "social" }
    fn availability(&self) -> SourceAvailability { SourceAvailability::CurrentOnly }
}

#[async_trait]
impl SentimentSource for SocialProvider {
    async fn sentiment(&self, request: &DataRequest) -> Result<DataStatus<SentimentSnapshot>, DataError> {
        if request.as_of < self.current_date {
            return Ok(DataStatus::WithheldHistorical { source: self.source_id().into(), as_of: request.as_of });
        }
        let response = self.http.execute(
            HttpRequest::get(&self.base_url, "/sentiment").with_query("symbol", request.symbol.as_str()),
        ).await;
        match response {
            Ok(value) => Ok(DataStatus::Available(SentimentSnapshot { summary: format!("Social context: {value}") })),
            Err(error) => Ok(DataStatus::Unavailable { source: self.source_id().into(), reason: error.to_string() }),
        }
    }
}
