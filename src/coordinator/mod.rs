use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

pub mod redis;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub execution_id: Uuid,
    pub job_id: Uuid,
    pub label_selector: String,
    pub selected_worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub execution_id: Uuid,
    pub worker_id: String,
    pub attempt_no: i32,
}

#[async_trait]
pub trait Coordinator: Send + Sync {
    async fn try_lock(&self, key: &str, ttl: Duration) -> Result<bool>;
    async fn heartbeat(&self, heartbeat: WorkerHeartbeat, ttl: Duration) -> Result<()>;
    async fn is_worker_live(&self, worker_id: &str) -> Result<bool>;
    async fn request_cancel(&self, execution_id: Uuid, ttl: Duration) -> Result<()>;
    async fn is_cancel_requested(&self, execution_id: Uuid) -> Result<bool>;
}

#[async_trait]
pub trait DispatchQueue: Send + Sync {
    async fn enqueue(&self, item: QueueItem) -> Result<()>;
    async fn claim_for_worker(
        &self,
        worker_id: &str,
        labels: &std::collections::BTreeMap<String, String>,
        lease_ttl: Duration,
    ) -> Result<Option<Lease>>;
    async fn queue_depth(&self, queue: &str) -> Result<usize>;
}
