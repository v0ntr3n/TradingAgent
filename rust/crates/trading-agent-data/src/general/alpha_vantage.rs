use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, MarketDataSource,
    MarketSnapshot, SourceAvailability,
};

pub struct AlphaVantageProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
    api_key: String,
}

impl AlphaVantageProvider {
    pub fn new(
        http: Arc<dyn HttpTransport>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

impl DataSource for AlphaVantageProvider {
    fn source_id(&self) -> &str {
        "alpha_vantage"
    }
    fn availability(&self) -> SourceAvailability {
        SourceAvailability::Archival
    }
}

#[async_trait]
impl MarketDataSource for AlphaVantageProvider {
    async fn market_snapshot(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<MarketSnapshot>, DataError> {
        let response = self
            .http
            .execute(
                HttpRequest::get(&self.base_url, "/query")
                    .with_query("function", "GLOBAL_QUOTE")
                    .with_query("symbol", request.symbol.as_str())
                    .with_query("apikey", &self.api_key),
            )
            .await?;
        if let Some(message) = response.get("Error Message").and_then(|v| v.as_str()) {
            return Err(DataError::Vendor {
                vendor: self.source_id().into(),
                message: message.into(),
            });
        }
        if let Some(message) = response.get("Note").and_then(|v| v.as_str()) {
            return Ok(DataStatus::Unavailable {
                source: self.source_id().into(),
                reason: message.into(),
            });
        }
        let quote = response.get("Global Quote").cloned().unwrap_or(response);
        Ok(DataStatus::Available(MarketSnapshot {
            summary: format!("Alpha Vantage quote: {quote}"),
        }))
    }
}
