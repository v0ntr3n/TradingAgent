use std::sync::Arc;

use crate::{
    LlmClient, LlmError, LlmTransport, ProviderConfig,
    providers::{anthropic, azure, bedrock, google, openai_compatible},
};

pub struct ProviderRegistry {
    transport: Arc<dyn LlmTransport>,
}

impl ProviderRegistry {
    pub fn new(transport: Arc<dyn LlmTransport>) -> Self { Self { transport } }

    pub fn create(&self, config: &ProviderConfig) -> Result<Arc<dyn LlmClient>, LlmError> {
        let provider = config.provider.trim().to_ascii_lowercase();
        match provider.as_str() {
            "openai" | "openai_compatible" | "qwen" | "dashscope" | "gitee" | "openrouter" => {
                Ok(Arc::new(openai_compatible::client(config.clone(), self.transport.clone())?))
            }
            "anthropic" => Ok(Arc::new(anthropic::client(config.clone(), self.transport.clone())?)),
            "google" | "gemini" => Ok(Arc::new(google::client(config.clone(), self.transport.clone())?)),
            "azure" | "azure_openai" => Ok(Arc::new(azure::client(config.clone(), self.transport.clone())?)),
            "bedrock" | "aws_bedrock" => Ok(Arc::new(bedrock::client(config.clone(), self.transport.clone())?)),
            _ => Err(LlmError::UnsupportedProvider(config.provider.clone())),
        }
    }
}
