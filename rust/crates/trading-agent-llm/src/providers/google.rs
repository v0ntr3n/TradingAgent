use std::sync::Arc;

use crate::{LlmError, LlmTransport, ProviderConfig};
use super::{Flavor, HttpProviderClient};

pub(crate) fn client(config: ProviderConfig, transport: Arc<dyn LlmTransport>) -> Result<HttpProviderClient, LlmError> {
    HttpProviderClient::new("google", Flavor::Google, config, "https://generativelanguage.googleapis.com", transport)
}
