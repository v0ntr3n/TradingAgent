use std::sync::Arc;

use crate::{LlmError, LlmTransport, ProviderConfig};
use super::{Flavor, HttpProviderClient};

pub(crate) fn client(config: ProviderConfig, transport: Arc<dyn LlmTransport>) -> Result<HttpProviderClient, LlmError> {
    HttpProviderClient::new("bedrock", Flavor::Bedrock, config, "https://bedrock-runtime.us-east-1.amazonaws.com", transport)
}
