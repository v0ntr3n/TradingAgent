use async_trait::async_trait;
use serde_json::Value;

use crate::DataError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub base_url: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

impl HttpRequest {
    pub fn get(base_url: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            base_url: base_url.into(),
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn post(base_url: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            base_url: base_url.into(),
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn with_query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.query.push((key.into(), value.to_string()));
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn with_json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<Value, DataError>;
}

#[derive(Clone, Default)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn execute(&self, request: HttpRequest) -> Result<Value, DataError> {
        let url = format!("{}{}", request.base_url.trim_end_matches('/'), request.path);
        let mut builder = match request.method {
            HttpMethod::Get => self.client.get(&url),
            HttpMethod::Post => self.client.post(&url),
        };
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        for (key, value) in request.headers {
            builder = builder.header(&key, &value);
        }
        if let Some(body) = request.body {
            builder = builder.json(&body);
        }

        let response = builder.send().await.map_err(|error| DataError::Vendor {
            vendor: "http".into(),
            message: if error.is_timeout() {
                "request timed out".into()
            } else {
                "request failed".into()
            },
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(DataError::Vendor {
                vendor: "http".into(),
                message: format!("HTTP status {status}"),
            });
        }
        response
            .json::<Value>()
            .await
            .map_err(|_| DataError::Vendor {
                vendor: "http".into(),
                message: "response was not valid JSON".into(),
            })
    }
}
