//! Point-in-time-safe data contracts for TradingAgent.

mod error;
mod policy;
mod router;
mod traits;
mod types;

pub use error::DataError;
pub use policy::DataPolicy;
pub use router::VendorRouter;
pub use traits::{
    FundamentalsSource, MacroSource, MarketDataSource, NewsSource, SentimentSource,
    TechnicalIndicatorSource,
};
pub use types::{
    DataAccess, DataRequest, DataStatus, FundamentalsSnapshot, MacroSnapshot, MarketSnapshot,
    NewsBatch, SentimentSnapshot, SourceAvailability, TechnicalIndicators,
};
