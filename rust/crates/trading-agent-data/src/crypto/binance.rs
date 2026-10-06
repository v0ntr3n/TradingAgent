use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde_json::Value;

use super::common::{binance_symbol, historical_withheld, scalar};
use crate::{
    DataError, DataRequest, DataSource, DataStatus, HttpRequest, HttpTransport, MarketDataSource,
    MarketSnapshot, SourceAvailability,
};

const RATIO_PERIODS: &[&str] = &["5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d"];

pub struct BinanceClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    now: DateTime<Utc>,
    timezone: Tz,
}

impl BinanceClient {
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

    pub async fn market_snapshot(
        &self,
        request: &DataRequest,
        interval: &str,
    ) -> Result<DataStatus<MarketSnapshot>, DataError> {
        if let Some(withheld) =
            historical_withheld("binance-live", request, self.now, self.timezone)
        {
            return Ok(withheld);
        }

        let pair = binance_symbol(&request.symbol);
        let ratio_period = if RATIO_PERIODS.contains(&interval) {
            interval
        } else {
            "15m"
        };

        let klines = self
            .transport
            .execute(
                HttpRequest::get(&self.base_url, "/fapi/v1/klines")
                    .with_query("symbol", &pair)
                    .with_query("interval", interval)
                    .with_query("limit", 24),
            )
            .await?;
        let depth = self
            .transport
            .execute(
                HttpRequest::get(&self.base_url, "/fapi/v1/depth")
                    .with_query("symbol", &pair)
                    .with_query("limit", 20),
            )
            .await?;
        let ticker = self
            .transport
            .execute(
                HttpRequest::get(&self.base_url, "/fapi/v1/ticker/24hr")
                    .with_query("symbol", &pair),
            )
            .await?;

        let mut ratio_results = Vec::new();
        for (path, value_key) in [
            ("/futures/data/topLongShortPositionRatio", "longShortRatio"),
            ("/futures/data/topLongShortAccountRatio", "longShortRatio"),
            (
                "/futures/data/globalLongShortAccountRatio",
                "longShortRatio",
            ),
            ("/futures/data/takerlongshortRatio", "buySellRatio"),
        ] {
            let value = self
                .transport
                .execute(
                    HttpRequest::get(&self.base_url, path)
                        .with_query("symbol", &pair)
                        .with_query("period", ratio_period)
                        .with_query("limit", 12),
                )
                .await?;
            ratio_results.push((path, value_key, value));
        }

        let mut lines = vec![
            format!("# Binance USD-M futures snapshot — {pair}"),
            format!("Interval: {ratio_period}"),
            format!(
                "Last: {} | Change: {}% | High: {} | Low: {} | Quote volume: {}",
                scalar(&ticker["lastPrice"]),
                scalar(&ticker["priceChangePercent"]),
                scalar(&ticker["highPrice"]),
                scalar(&ticker["lowPrice"]),
                scalar(&ticker["quoteVolume"]),
            ),
        ];

        if let (Some(bid), Some(ask)) = (
            depth
                .get("bids")
                .and_then(Value::as_array)
                .and_then(|rows| rows.first()),
            depth
                .get("asks")
                .and_then(Value::as_array)
                .and_then(|rows| rows.first()),
        ) {
            lines.push(format!(
                "Best bid: {} ({}) | Best ask: {} ({})",
                scalar(&bid[0]),
                scalar(&bid[1]),
                scalar(&ask[0]),
                scalar(&ask[1])
            ));
        }

        if let Some(rows) = klines.as_array() {
            for row in rows.iter().rev().take(12).rev() {
                if let Some(row) = row.as_array() {
                    if row.len() >= 6 {
                        lines.push(format!(
                            "Candle {}: O {} H {} L {} C {} V {}",
                            scalar(&row[0]),
                            scalar(&row[1]),
                            scalar(&row[2]),
                            scalar(&row[3]),
                            scalar(&row[4]),
                            scalar(&row[5])
                        ));
                    }
                }
            }
        }

        for (path, key, value) in ratio_results {
            if let Some(rows) = value.as_array() {
                for row in rows.iter().rev().take(8).rev() {
                    lines.push(format!("{path}: {}", scalar(&row[key])));
                }
            }
        }

        Ok(DataStatus::Available(MarketSnapshot {
            summary: lines.join("\n"),
        }))
    }
}

impl DataSource for BinanceClient {
    fn source_id(&self) -> &str {
        "binance"
    }

    fn availability(&self) -> SourceAvailability {
        SourceAvailability::CurrentOnly
    }
}

#[async_trait]
impl MarketDataSource for BinanceClient {
    async fn market_snapshot(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<MarketSnapshot>, DataError> {
        BinanceClient::market_snapshot(self, request, "15m").await
    }
}
