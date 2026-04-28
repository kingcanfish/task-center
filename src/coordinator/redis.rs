use super::{Coordinator, DispatchQueue, Lease, QueueItem};
use crate::domain::labels::LabelSelector;
use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisCoordinator {
    manager: ConnectionManager,
}

impl RedisCoordinator {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let manager = client.get_connection_manager().await?;
        Ok(Self { manager })
    }
}

#[async_trait]
impl Coordinator for RedisCoordinator {
    async fn try_lock(&self, key: &str, ttl: Duration) -> Result<bool> {
        let mut conn = self.manager.clone();
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut conn)
            .await?;
        Ok(result.is_some())
    }

    async fn heartbeat(&self, heartbeat: WorkerHeartbeat, ttl: Duration) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = format!("worker:{}", heartbeat.worker_id);
        let value = serde_json::to_string(&heartbeat)?;
        let _: () = conn.set_ex(key, value, ttl.as_secs().max(1)).await?;
        Ok(())
    }

    async fn is_worker_live(&self, worker_id: &str) -> Result<bool> {
        let mut conn = self.manager.clone();
        let key = format!("worker:{worker_id}");
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }

    async fn request_cancel(&self, execution_id: Uuid, ttl: Duration) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = format!("cancel:{execution_id}");
        let _: () = conn.set_ex(key, "1", ttl.as_secs().max(1)).await?;
        Ok(())
    }

    async fn is_cancel_requested(&self, execution_id: Uuid) -> Result<bool> {
        let mut conn = self.manager.clone();
        let key = format!("cancel:{execution_id}");
        Ok(conn.exists(key).await?)
    }
}

#[async_trait]
impl DispatchQueue for RedisCoordinator {
    async fn enqueue(&self, item: QueueItem) -> Result<()> {
        let mut conn = self.manager.clone();
        let queue = item
            .selected_worker_id
            .as_ref()
            .map(|worker_id| format!("queue:worker:{worker_id}"))
            .unwrap_or_else(|| "queue:shared".to_string());
        let value = serde_json::to_string(&item)?;
        let _: () = conn.rpush(queue, value).await?;
        Ok(())
    }

    async fn claim_for_worker(
        &self,
        worker_id: &str,
        labels: &BTreeMap<String, String>,
        lease_ttl: Duration,
    ) -> Result<Option<Lease>> {
        let queues = [
            format!("queue:worker:{worker_id}"),
            "queue:shared".to_string(),
        ];
        let mut conn = self.manager.clone();

        for queue in queues {
            let value: Option<String> = conn.lpop(&queue, None).await?;
            let Some(value) = value else { continue };
            let item: QueueItem = serde_json::from_str(&value)?;

            if !LabelSelector::parse(&item.label_selector)?.matches(labels) {
                let _: () = conn.rpush("queue:shared", value).await?;
                continue;
            }

            let lease = Lease {
                execution_id: item.execution_id,
                worker_id: worker_id.to_string(),
                attempt_no: 1,
            };
            let lease_key = format!("lease:{}", item.execution_id);
            let _: () = conn
                .set_ex(
                    lease_key,
                    serde_json::to_string(&lease)?,
                    lease_ttl.as_secs().max(1),
                )
                .await?;
            return Ok(Some(lease));
        }

        Ok(None)
    }

    async fn queue_depth(&self, queue: &str) -> Result<usize> {
        let mut conn = self.manager.clone();
        let len: usize = conn.llen(queue).await?;
        Ok(len)
    }
}
