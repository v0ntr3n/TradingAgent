use std::sync::Arc;

use super::{Flavor, HttpProviderClient};
use crate::{LlmError, LlmTransport, ProviderConfig};

pub(crate) fn client(
    config: ProviderConfig,
    transport: Arc<dyn LlmTransport>,
) -> Result<HttpProviderClient, LlmError> {
    HttpProviderClient::new(
        "bedrock",
        Flavor::Bedrock,
        config,
        "https://bedrock-runtime.us-east-1.amazonaws.com",
        transport,
    )
}
