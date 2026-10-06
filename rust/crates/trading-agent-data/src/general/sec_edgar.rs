use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{
    DataError, DataRequest, DataSource, DataStatus, FundamentalsSnapshot, FundamentalsSource,
    HttpRequest, HttpTransport, SourceAvailability,
};

pub struct SecEdgarProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
}

impl SecEdgarProvider {
    pub fn new(http: Arc<dyn HttpTransport>, base_url: impl Into<String>) -> Self {
        Self { http, base_url: base_url.into() }
    }
}

impl DataSource for SecEdgarProvider {
    fn source_id(&self) -> &str { "sec_edgar" }
    fn availability(&self) -> SourceAvailability { SourceAvailability::Archival }
}

#[async_trait]
impl FundamentalsSource for SecEdgarProvider {
    async fn fundamentals(&self, request: &DataRequest) -> Result<DataStatus<FundamentalsSnapshot>, DataError> {
        let response = self.http.execute(
            HttpRequest::get(&self.base_url, format!("/submissions/{}.json", request.symbol.as_str()))
                .with_header("User-Agent", "TradingAgent/0.1"),
        ).await?;
        let filings = response.get("filings").and_then(|v| v.as_array()).ok_or_else(|| DataError::Vendor {
            vendor: "sec_edgar".into(), message: "missing filings".into(),
        })?;
        let mut known = Vec::new();
        for filing in filings {
            let Some(date) = filing.get("filed_at").and_then(|v| v.as_str()).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) else { continue };
            if date > request.as_of { continue; }
            let form = filing.get("form").and_then(|v| v.as_str()).unwrap_or("filing");
            let summary = filing.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            known.push(format!("{date} {form}: {summary}"));
        }
        if known.is_empty() {
            return Ok(DataStatus::Unavailable { source: self.source_id().into(), reason: "no filings known by as-of date".into() });
        }
        Ok(DataStatus::Available(FundamentalsSnapshot { summary: known.join("\n") }))
    }
}
