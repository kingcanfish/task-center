use crate::domain::state::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Http,
    Shell,
    Builtin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    Ignore,
    FireOnceNow,
    CatchUpAll,
    CatchUpWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    Random,
    RoundRobin,
    LeastActive,
    Failover,
    Busyover,
    ConsistentHash,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    AllowParallel,
    SkipIfRunning,
    QueueIfRunning,
    FailIfRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub paused: bool,
    pub task_type: TaskType,
    pub config_json: Value,
    pub cron_expr: String,
    pub timezone: String,
    #[serde(with = "datetime_utc_rfc3339")]
    pub next_fire_at: DateTime<Utc>,
    pub misfire_policy: MisfirePolicy,
    pub misfire_grace_seconds: i64,
    pub max_retries: i32,
    pub retry_delay_seconds: i64,
    pub timeout_seconds: i64,
    pub label_selector: String,
    pub route_strategy: RouteStrategy,
    pub concurrency_policy: ConcurrencyPolicy,
    #[serde(with = "datetime_utc_rfc3339")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "datetime_utc_rfc3339")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Execution {
    pub id: Uuid,
    pub job_id: Uuid,
    #[serde(with = "datetime_utc_rfc3339")]
    pub scheduled_at: DateTime<Utc>,
    pub manual_trigger_id: Option<Uuid>,
    pub status: ExecutionStatus,
    pub idempotency_key: String,
    pub attempt_count: i32,
    pub shard_index: i32,
    pub shard_total: i32,
    pub selected_worker_id: Option<String>,
    #[serde(with = "option_datetime_utc_rfc3339")]
    pub next_retry_at: Option<DateTime<Utc>>,
    #[serde(with = "datetime_utc_rfc3339")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "datetime_utc_rfc3339")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionAttempt {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub worker_id: String,
    #[serde(with = "datetime_utc_rfc3339")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "option_datetime_utc_rfc3339")]
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub status: ExecutionStatus,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerSnapshot {
    pub worker_id: String,
    pub name: Option<String>,
    pub labels: Value,
    pub capacity: i32,
    #[serde(with = "datetime_utc_rfc3339")]
    pub last_seen_snapshot: DateTime<Utc>,
    pub status_snapshot: Value,
    #[serde(with = "datetime_utc_rfc3339")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "datetime_utc_rfc3339")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHeartbeat {
    pub worker_id: String,
    pub labels: BTreeMap<String, String>,
    pub capacity: usize,
    pub active_count: usize,
}

mod datetime_utc_rfc3339 {
    use super::*;
    use serde::de::Error;

    pub fn serialize<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_rfc3339())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(D::Error::custom)
    }
}

mod option_datetime_utc_rfc3339 {
    use super::*;
    use serde::de::Error;

    pub fn serialize<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_rfc3339()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        value
            .map(|value| {
                DateTime::parse_from_rfc3339(&value)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(D::Error::custom)
            })
            .transpose()
    }
}
