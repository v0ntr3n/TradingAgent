use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::common::{historical_withheld, scalar};
use crate::{DataError, DataRequest, DataStatus, HttpRequest, HttpTransport, NewsBatch};

pub struct BlockBeatsClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl BlockBeatsClient {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        base_url: impl Into<String>,
        now: DateTime<Utc>,
        timezone: Tz,
    ) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            now,
            timezone,
        }
    }

    pub async fn news(
        &self,
        request: &DataRequest,
        limit: usize,
    ) -> Result<DataStatus<NewsBatch>, DataError> {
        if let Some(withheld) =
            historical_withheld("blockbeats-news-live", request, self.now, self.timezone)
        {
            return Ok(withheld);
        }
        let limit = limit.clamp(1, 30);
        let data = self
            .transport
            .execute(
                HttpRequest::get(&self.base_url, "/v1/open-api/open-flash")
                    .with_query("page", 1)
                    .with_query("size", limit)
                    .with_query("type", "push")
                    .with_query("lang", "cn"),
            )
            .await?;
        let rows = data
            .get("data")
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| DataError::Vendor {
                vendor: "blockbeats".into(),
                message: "no articles returned".into(),
            })?;
        let items = rows
            .iter()
            .take(limit)
            .map(|row| {
                format!(
                    "**{}** ({}) — {}",
                    scalar(&row["title"]),
                    scalar(&row["create_time"]),
                    scalar(&row["content"])
                )
            })
            .collect();
        Ok(DataStatus::Available(NewsBatch { items }))
    }
}
