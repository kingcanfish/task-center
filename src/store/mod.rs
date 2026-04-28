use crate::domain::state::ExecutionStatus;
use crate::domain::types::{Execution, ExecutionAttempt, Job, WorkerSnapshot};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

pub mod postgres;

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn list_enabled_due(&self, now: DateTime<Utc>) -> Result<Vec<Job>>;
    async fn get(&self, id: Uuid) -> Result<Option<Job>>;
    async fn create(&self, job: CreateJob) -> Result<Job>;
    async fn update_next_fire_at(&self, id: Uuid, next_fire_at: DateTime<Utc>) -> Result<()>;
    async fn set_paused(&self, id: Uuid, paused: bool) -> Result<()>;
}

#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn create_execution(&self, input: CreateExecution) -> Result<Execution>;
    async fn get_execution(&self, id: Uuid) -> Result<Option<Execution>>;
    async fn update_status(&self, id: Uuid, status: ExecutionStatus) -> Result<()>;
    async fn create_attempt(&self, input: CreateAttempt) -> Result<ExecutionAttempt>;
    async fn finish_attempt(&self, input: FinishAttempt) -> Result<()>;
    async fn list_retry_due(&self, now: DateTime<Utc>) -> Result<Vec<Execution>>;
}

#[async_trait]
pub trait WorkerRepository: Send + Sync {
    async fn upsert_snapshot(&self, snapshot: UpsertWorkerSnapshot) -> Result<()>;
    async fn list_snapshots(&self) -> Result<Vec<WorkerSnapshot>>;
}

#[derive(Debug, Clone)]
pub struct CreateJob {
    pub name: String,
    pub task_type: crate::domain::types::TaskType,
    pub config_json: Value,
    pub cron_expr: String,
    pub next_fire_at: DateTime<Utc>,
    pub label_selector: String,
}

#[derive(Debug, Clone)]
pub struct CreateExecution {
    pub job_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub manual_trigger_id: Option<Uuid>,
    pub idempotency_key: String,
    pub shard_index: i32,
    pub shard_total: i32,
}

#[derive(Debug, Clone)]
pub struct CreateAttempt {
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub worker_id: String,
}

#[derive(Debug, Clone)]
pub struct FinishAttempt {
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpsertWorkerSnapshot {
    pub worker_id: String,
    pub labels: Value,
    pub capacity: i32,
    pub status_snapshot: Value,
}
