use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{LlmClient, LlmError, LlmRequest};

pub async fn complete_structured<T>(
    client: &dyn LlmClient,
    request: LlmRequest,
) -> Result<T, LlmError>
where
    T: DeserializeOwned + JsonSchema,
{
    let schema = schemars::schema_for!(T);
    let value = client.complete_json(request, &schema).await?;
    serde_json::from_value(value).map_err(|error| LlmError::Structured(error.to_string()))
}
