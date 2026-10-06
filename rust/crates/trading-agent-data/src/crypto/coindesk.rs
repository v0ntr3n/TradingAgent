use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::common::{crypto_base, historical_withheld, scalar};
use crate::{DataError, DataRequest, DataStatus, HttpRequest, HttpTransport, NewsBatch};

pub struct CoinDeskClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    api_key: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl CoinDeskClient {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        now: DateTime<Utc>,
        timezone: Tz,
    ) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            api_key: api_key.into(),
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
            historical_withheld("coindesk-news-live", request, self.now, self.timezone)
        {
            return Ok(withheld);
        }
        let limit = limit.clamp(1, 50);
        let data = self
            .transport
            .execute(
                HttpRequest::get(&self.base_url, "/news/v1/article/list")
                    .with_query("lang", "EN")
                    .with_query("limit", limit)
                    .with_query("api_key", &self.api_key)
                    .with_query("categories", crypto_base(&request.symbol)),
            )
            .await?;
        let rows = data
            .get("Data")
            .and_then(|value| value.as_array())
            .ok_or_else(|| DataError::Vendor {
                vendor: "coindesk".into(),
                message: "no articles returned".into(),
            })?;
        let items = rows
            .iter()
            .take(limit)
            .map(|row| {
                format!(
                    "**{}** [{}] — {} {}",
                    scalar(&row["TITLE"]),
                    scalar(&row["SENTIMENT"]),
                    scalar(&row["BODY"]),
                    scalar(&row["URL"])
                )
            })
            .collect();
        Ok(DataStatus::Available(NewsBatch { items }))
    }
}
