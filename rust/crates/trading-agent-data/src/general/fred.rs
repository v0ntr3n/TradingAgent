use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, MacroSnapshot,
    MacroSource, SourceAvailability,
};

pub struct FredProvider {
    http: Arc<dyn HttpTransport>,
    base_url: String,
    api_key: String,
}

impl FredProvider {
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

impl DataSource for FredProvider {
    fn source_id(&self) -> &str {
        "fred"
    }
    fn availability(&self) -> SourceAvailability {
        SourceAvailability::Archival
    }
}

#[async_trait]
impl MacroSource for FredProvider {
    async fn macro_snapshot(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<MacroSnapshot>, DataError> {
        let response = self
            .http
            .execute(
                HttpRequest::get(&self.base_url, "/fred/series/observations")
                    .with_query("series_id", "FEDFUNDS")
                    .with_query("api_key", &self.api_key)
                    .with_query("file_type", "json")
                    .with_query("observation_end", request.as_of.format("%Y-%m-%d")),
            )
            .await?;
        let observations = response
            .get("observations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DataError::Vendor {
                vendor: self.source_id().into(),
                message: "missing observations".into(),
            })?;
        let summary = observations
            .iter()
            .map(|item| {
                format!(
                    "{}={}",
                    item.get("date").and_then(|v| v.as_str()).unwrap_or("?"),
                    item.get("value").and_then(|v| v.as_str()).unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(DataStatus::Available(MacroSnapshot {
            summary: format!("FRED: {summary}"),
        }))
    }
}
