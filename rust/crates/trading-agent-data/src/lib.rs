//! Point-in-time-safe data contracts for TradingAgent.

pub mod crypto;
mod error;
pub mod general;
mod http;
mod policy;
mod router;
mod traits;
mod types;

pub use error::DataError;
pub use http::{HttpMethod, HttpRequest, HttpTransport, ReqwestTransport};
pub use policy::DataPolicy;
pub use router::VendorRouter;
pub use traits::{
    DataSource, FundamentalsSource, MacroSource, MarketDataSource, NewsSource, SentimentSource,
    TechnicalIndicatorSource,
};
pub use types::{
    DataAccess, DataRequest, DataStatus, FundamentalsSnapshot, MacroSnapshot, MarketSnapshot,
    NewsBatch, SentimentSnapshot, SourceAvailability, TechnicalIndicators,
};
