pub mod bugutv;
pub mod builtin;
pub mod http;
pub mod shell;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub execution_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub shard_index: i32,
    pub shard_total: i32,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub exit_code: Option<i32>,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, config: Value, context: ExecutionContext) -> Result<TaskOutput>;
}
