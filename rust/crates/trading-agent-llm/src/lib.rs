//! Provider-neutral LLM contracts for the native Rust port.

mod client;
pub mod providers;
mod registry;
mod research;
mod structured;
mod types;

pub use client::{LlmClient, LlmTransport, ReqwestLlmTransport};
pub use registry::ProviderRegistry;
pub use research::{ResearchClient, ResearchRequest, ResearchResponse, SearchResearchClient};
pub use structured::complete_structured;
pub use types::{
    LlmError, LlmHttpRequest, LlmMessage, LlmRequest, LlmResponse, LlmRole, ProviderConfig,
};
