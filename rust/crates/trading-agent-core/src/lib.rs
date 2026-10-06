//! Native Rust core for TradingAgent.

pub mod agents;
pub mod checkpoint;
mod config;
mod decision;
mod error;
pub mod events;
mod external;
mod portfolio;
mod preferences;
mod state;
mod symbol;
pub mod workflow;

pub use config::{ModelTierConfig, RunConfig};
pub use decision::{PositionSizing, TraderAction, TraderProposal};
pub use error::CoreError;
pub use external::ExternalReport;
pub use portfolio::{PortfolioContext, Position};
pub use preferences::{InvestmentPreferences, RiskStyle, VenuePreference};
pub use state::{AgentState, SCHEMA_VERSION};
pub use symbol::{AssetType, Symbol};
