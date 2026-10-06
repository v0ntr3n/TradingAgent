use std::sync::Arc;

use crate::{LlmError, LlmTransport, ProviderConfig};
use super::{Flavor, HttpProviderClient};

pub(crate) fn client(config: ProviderConfig, transport: Arc<dyn LlmTransport>) -> Result<HttpProviderClient, LlmError> {
    if config.base_url.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(LlmError::InvalidConfig("azure requires base_url".into()));
    }
    HttpProviderClient::new("azure", Flavor::Azure, config, "", transport)
}
