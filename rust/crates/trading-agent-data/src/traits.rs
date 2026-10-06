use async_trait::async_trait;

use crate::{
    DataError, DataRequest, DataStatus, FundamentalsSnapshot, MacroSnapshot, MarketSnapshot,
    NewsBatch, SentimentSnapshot, SourceAvailability, TechnicalIndicators,
};

pub trait DataSource: Send + Sync {
    fn source_id(&self) -> &str;
    fn availability(&self) -> SourceAvailability;
}

#[async_trait]
pub trait MarketDataSource: DataSource {
    async fn market_snapshot(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<MarketSnapshot>, DataError>;
}

#[async_trait]
pub trait TechnicalIndicatorSource: DataSource {
    async fn indicators(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<TechnicalIndicators>, DataError>;
}

#[async_trait]
pub trait NewsSource: DataSource {
    async fn news(&self, request: &DataRequest) -> Result<DataStatus<NewsBatch>, DataError>;
}

#[async_trait]
pub trait SentimentSource: DataSource {
    async fn sentiment(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<SentimentSnapshot>, DataError>;
}

#[async_trait]
pub trait FundamentalsSource: DataSource {
    async fn fundamentals(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<FundamentalsSnapshot>, DataError>;
}

#[async_trait]
pub trait MacroSource: DataSource {
    async fn macro_snapshot(
        &self,
        request: &DataRequest,
    ) -> Result<DataStatus<MacroSnapshot>, DataError>;
}
