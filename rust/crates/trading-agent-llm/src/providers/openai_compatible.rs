use std::sync::Arc;

use crate::{LlmError, LlmTransport, ProviderConfig};
use super::{Flavor, HttpProviderClient};

pub(crate) fn client(config: ProviderConfig, transport: Arc<dyn LlmTransport>) -> Result<HttpProviderClient, LlmError> {
    let default = match config.provider.to_ascii_lowercase().as_str() {
        "qwen" | "dashscope" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        _ => "https://api.openai.com/v1",
    };
    HttpProviderClient::new("openai_compatible", Flavor::OpenAiCompatible, config, default, transport)
}
