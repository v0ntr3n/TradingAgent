use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{LlmClient, LlmError, LlmMessage, LlmRequest};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResearchRequest {
    pub query: String,
    pub as_of: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResearchResponse {
    pub content: String,
}

#[async_trait]
pub trait ResearchClient: Send + Sync {
    async fn research(&self, request: ResearchRequest) -> Result<ResearchResponse, LlmError>;
}

pub struct SearchResearchClient {
    client: Arc<dyn LlmClient>,
    current_date: NaiveDate,
}

impl SearchResearchClient {
    pub fn new(client: Arc<dyn LlmClient>, current_date: NaiveDate) -> Self {
        Self {
            client,
            current_date,
        }
    }
}

#[async_trait]
impl ResearchClient for SearchResearchClient {
    async fn research(&self, request: ResearchRequest) -> Result<ResearchResponse, LlmError> {
        if request.as_of < self.current_date {
            return Err(LlmError::HistoricalSearchRefused {
                as_of: request.as_of,
            });
        }
        let mut llm_request = LlmRequest::new(vec![LlmMessage::user(request.query)]);
        llm_request.enable_search = true;
        let response = self.client.complete(llm_request).await?;
        Ok(ResearchResponse {
            content: response.content,
        })
    }
}
