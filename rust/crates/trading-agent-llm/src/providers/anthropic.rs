use std::sync::Arc;

use super::{Flavor, HttpProviderClient};
use crate::{LlmError, LlmTransport, ProviderConfig};

pub(crate) fn client(
    config: ProviderConfig,
    transport: Arc<dyn LlmTransport>,
) -> Result<HttpProviderClient, LlmError> {
    HttpProviderClient::new(
        "anthropic",
        Flavor::Anthropic,
        config,
        "https://api.anthropic.com",
        transport,
    )
}
