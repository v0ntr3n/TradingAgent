use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde_json::json;

use super::common::{historical_withheld, taapi_symbol};
use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, SourceAvailability,
    TechnicalIndicatorSource, TechnicalIndicators,
};

const INDICATORS: &[&str] = &[
    "ema", "ichimoku", "supertrend", "donchianchannels", "macd", "rsi", "stochrsi",
    "trix", "stc", "vwap", "atr", "bbands", "keltnerchannels", "chop", "engulfing",
    "hammer", "morningstar", "eveningstar", "3whitesoldiers", "3blackcrows",
];

pub struct TaapiClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    api_key: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl TaapiClient {
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

    pub async fn indicators(
        &self,
        request: &DataRequest,
        interval: &str,
    ) -> Result<DataStatus<TechnicalIndicators>, DataError> {
        if let Some(withheld) = historical_withheld("taapi-live", request, self.now, self.timezone) {
            return Ok(withheld);
        }
        let pair = taapi_symbol(&request.symbol);
        let indicators = INDICATORS
            .iter()
            .map(|indicator| json!({"id": indicator, "indicator": indicator}))
            .collect::<Vec<_>>();
        let body = json!({
            "constructs": [{
                "id": "crypto",
                "exchange": "binance",
                "symbol": pair,
                "timeframe": interval,
                "indicators": indicators,
            }]
        });
        let data = self
            .transport
            .execute(
                HttpRequest::post(&self.base_url, "/bulk")
                    .with_header("Authorization", format!("Bearer {}", self.api_key))
                    .with_json(body),
            )
            .await?;
        Ok(DataStatus::Available(TechnicalIndicators {
            summary: format!("# TAAPI bulk indicators — {pair} ({interval})\n{data}"),
        }))
    }
}

impl DataSource for TaapiClient {
    fn source_id(&self) -> &str {
        "taapi"
    }
    fn availability(&self) -> SourceAvailability {
        SourceAvailability::CurrentOnly
    }
}

#[async_trait]
impl TechnicalIndicatorSource for TaapiClient {
    async fn indicators(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<TechnicalIndicators>, DataError> {
        TaapiClient::indicators(self, request, "15m").await
    }
}
