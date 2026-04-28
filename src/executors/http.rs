use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;

use super::{ExecutionContext, TaskExecutor, TaskOutput};

const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 4096;

pub struct HttpExecutor {
    client: reqwest::Client,
}

impl HttpExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct HttpConfig {
    method: String,
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<serde_json::Value>,
    expected_statuses: Option<Vec<u16>>,
    timeout_seconds: Option<u64>,
}

#[async_trait]
impl TaskExecutor for HttpExecutor {
    async fn execute(
        &self,
        config: serde_json::Value,
        context: ExecutionContext,
    ) -> Result<TaskOutput> {
        let config: HttpConfig = serde_json::from_value(config)?;
        let method: Method = config.method.parse()?;
        let timeout =
            Duration::from_secs(config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS));

        let mut request = self.client.request(method, &config.url).timeout(timeout);
        for (name, value) in config.headers {
            request = request.header(name, value);
        }
        if let Some(body) = config.body {
            request = request.header(reqwest::header::CONTENT_TYPE, "application/json");
            request = request.body(serde_json::to_vec(&body)?);
        }

        let response = tokio::select! {
            response = request.send() => response?,
            _ = context.cancel.cancelled() => {
                return Ok(TaskOutput {
                    exit_code: None,
                    stdout_summary: None,
                    stderr_summary: None,
                    error_message: Some("http request cancelled".to_string()),
                });
            }
        };
        let status = response.status();
        let status_code = status.as_u16();
        let Some(body) =
            response_summary(response, DEFAULT_OUTPUT_LIMIT_BYTES, &context.cancel).await?
        else {
            return Ok(TaskOutput {
                exit_code: None,
                stdout_summary: None,
                stderr_summary: None,
                error_message: Some("http request cancelled".to_string()),
            });
        };
        let expected = config
            .expected_statuses
            .map(|statuses| statuses.contains(&status_code))
            .unwrap_or_else(|| status.is_success());

        Ok(TaskOutput {
            exit_code: Some(if expected { 0 } else { 1 }),
            stdout_summary: Some(body),
            stderr_summary: None,
            error_message: if expected {
                None
            } else {
                Some(format!("unexpected HTTP status {status_code}"))
            },
        })
    }
}

async fn response_summary(
    mut response: reqwest::Response,
    limit: usize,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Option<String>> {
    let mut stored = Vec::with_capacity(limit.min(DEFAULT_OUTPUT_LIMIT_BYTES));
    while stored.len() < limit {
        let Some(chunk) = (tokio::select! {
            chunk = response.chunk() => chunk?,
            _ = cancel.cancelled() => return Ok(None),
        }) else {
            break;
        };
        let remaining = limit - stored.len();
        stored.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    Ok(Some(summary_from_bytes(&stored)))
}

fn summary_from_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_string(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).to_string()
        }
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    }
}
