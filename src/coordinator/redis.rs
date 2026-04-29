use super::{Coordinator, DispatchQueue, Lease, QueueItem};
use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

const CLAIM_SCRIPT: &str = r#"
local direct_queue = KEYS[1]
local shared_queue = KEYS[2]
local worker_id = ARGV[1]
local labels_json = ARGV[2]
local lease_ttl = tonumber(ARGV[3])
local attempt_ttl = lease_ttl * 30
if attempt_ttl < 86400 then
    attempt_ttl = 86400
end

local ok_labels, labels = pcall(cjson.decode, labels_json)
if not ok_labels or type(labels) ~= 'table' then
    return redis.error_reply('invalid worker labels json')
end

local function trim(value)
    return string.match(value, '^%s*(.-)%s*$')
end

local function selector_matches(selector)
    if selector == nil then
        selector = ''
    end

    for part in string.gmatch(selector .. ',', '(.-),') do
        part = trim(part)
        if part ~= '' then
            local key, value = string.match(part, '^%s*([^=]+)%s*=%s*(.-)%s*$')
            if key == nil or value == nil then
                return nil, 'invalid label selector'
            end

            key = trim(key)
            value = trim(value)
            if key == '' or value == '' then
                return nil, 'invalid label selector'
            end

            if labels[key] ~= value then
                return false
            end
        end
    end

    return true
end

local function claim_from(queue)
    local len = redis.call('LLEN', queue)
    for _ = 1, len do
        local raw = redis.call('LINDEX', queue, 0)
        if raw == false then
            return nil
        end

        local ok_item, item = pcall(cjson.decode, raw)
        if not ok_item or type(item) ~= 'table' then
            return redis.error_reply('invalid queue item json')
        end

        local matches, selector_error = selector_matches(item['label_selector'])
        if selector_error ~= nil then
            return redis.error_reply(selector_error)
        end

        if matches == false then
            redis.call('LPOP', queue)
            redis.call('RPUSH', queue, raw)
        else
            local lease_key = 'lease:' .. item['execution_id']
            local attempt_key = 'attempt:' .. item['execution_id']
            local attempt_no = tonumber(redis.call('GET', attempt_key) or '0') + 1
            local lease = {
                execution_id = item['execution_id'],
                worker_id = worker_id,
                attempt_no = attempt_no
            }
            local lease_json = cjson.encode(lease)
            local leased = redis.call('SET', lease_key, lease_json, 'NX', 'EX', lease_ttl)
            redis.call('LPOP', queue)

            if leased then
                redis.call('SET', attempt_key, attempt_no, 'EX', attempt_ttl)
                return lease_json
            end
        end
    end

    return nil
end

local lease = claim_from(direct_queue)
if lease ~= nil then
    return lease
end

return claim_from(shared_queue)
"#;

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

    pub async fn list_worker_heartbeats(&self) -> Result<Vec<WorkerHeartbeat>> {
        let mut conn = self.manager.clone();
        let mut cursor = 0_u64;
        let mut heartbeats = Vec::new();

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg("worker:*")
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;

            for key in keys {
                let Some(value): Option<String> = conn.get(&key).await? else {
                    continue;
                };
                match serde_json::from_str::<WorkerHeartbeat>(&value) {
                    Ok(heartbeat) => heartbeats.push(heartbeat),
                    Err(err) => log::warn!("invalid worker heartbeat at {key}: {err}"),
                }
            }

            if next_cursor == 0 {
                break;
            }
            cursor = next_cursor;
        }

        heartbeats.sort_by(|left, right| left.worker_id.cmp(&right.worker_id));
        Ok(heartbeats)
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
        let mut conn = self.manager.clone();
        let direct_queue = format!("queue:worker:{worker_id}");
        let labels_json = serde_json::to_string(labels)?;
        let lease_json: Option<String> = redis::Script::new(CLAIM_SCRIPT)
            .key(direct_queue)
            .key("queue:shared")
            .arg(worker_id)
            .arg(labels_json)
            .arg(lease_ttl.as_secs().max(1))
            .invoke_async(&mut conn)
            .await?;

        lease_json
            .map(|lease_json| serde_json::from_str(&lease_json))
            .transpose()
            .map_err(Into::into)
    }

    async fn queue_depth(&self, queue: &str) -> Result<usize> {
        let mut conn = self.manager.clone();
        let len: usize = conn.llen(queue).await?;
        Ok(len)
    }
}
