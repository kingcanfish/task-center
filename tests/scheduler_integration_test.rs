mod common;

use chrono::{Duration, Utc};
use job_scheduler::coordinator::redis::RedisCoordinator;
use job_scheduler::coordinator::{DispatchQueue, Lease};
use job_scheduler::domain::types::TaskType;
use job_scheduler::scheduling::SchedulerTick;
use job_scheduler::store::postgres::PostgresStore;
use job_scheduler::store::{CreateExecution, CreateJob, ExecutionRepository, JobRepository};
use redis::aio::ConnectionManager;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration as StdDuration;
use uuid::Uuid;

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

async fn redis() -> RedisCoordinator {
    RedisCoordinator::connect(&redis_url()).await.unwrap()
}

async fn redis_connection() -> ConnectionManager {
    redis::Client::open(redis_url())
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
}

async fn delete_keys(keys: &[String]) {
    let mut conn = redis_connection().await;
    let _: usize = redis::cmd("DEL")
        .arg(keys)
        .query_async(&mut conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn two_scheduler_ticks_do_not_duplicate_execution() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let redis = redis().await;
    let test_label = Uuid::new_v4().to_string();
    let job = store
        .create(CreateJob {
            name: format!("due_job_{}", Uuid::new_v4()),
            task_type: TaskType::Http,
            config_json: json!({"url": "https://example.com"}),
            cron_expr: "0 0 8 * * *".to_string(),
            next_fire_at: Utc::now() - Duration::minutes(1),
            label_selector: format!("test={test_label}"),
        })
        .await
        .unwrap();

    let tick = SchedulerTick::new(store.clone(), redis.clone(), redis.clone());
    tick.run_once(Utc::now()).await.unwrap();
    tick.run_once(Utc::now()).await.unwrap();

    let executions = store.list_executions_for_job(job.id).await.unwrap();
    assert_eq!(executions.len(), 1);

    let lease = claim_for_label(&redis, "test", &test_label).await;
    assert_eq!(lease.execution_id, executions[0].id);
    delete_keys(&[format!("lease:{}", lease.execution_id)]).await;
}

#[tokio::test]
async fn scheduler_tick_recovers_existing_execution_by_enqueuing_it() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let redis = redis().await;
    let test_label = Uuid::new_v4().to_string();
    let scheduled_at = Utc::now() - Duration::minutes(1);
    let job = store
        .create(CreateJob {
            name: format!("recover_due_job_{}", Uuid::new_v4()),
            task_type: TaskType::Http,
            config_json: json!({"url": "https://example.com"}),
            cron_expr: "0 0 8 * * *".to_string(),
            next_fire_at: scheduled_at,
            label_selector: format!("test={test_label}"),
        })
        .await
        .unwrap();
    let existing = store
        .create_execution(CreateExecution {
            job_id: job.id,
            scheduled_at: job.next_fire_at,
            manual_trigger_id: None,
            idempotency_key: format!("{}:{}:0", job.id, job.next_fire_at.timestamp()),
            shard_index: 0,
            shard_total: 1,
        })
        .await
        .unwrap();

    let tick = SchedulerTick::new(store.clone(), redis.clone(), redis.clone());
    tick.run_once(Utc::now()).await.unwrap();

    let executions = store.list_executions_for_job(job.id).await.unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].id, existing.id);

    let lease = claim_for_label(&redis, "test", &test_label).await;
    assert_eq!(lease.execution_id, existing.id);
    delete_keys(&[format!("lease:{}", lease.execution_id)]).await;
}

async fn claim_for_label(redis: &RedisCoordinator, key: &str, value: &str) -> Lease {
    let labels = BTreeMap::from([(key.to_string(), value.to_string())]);
    redis
        .claim_for_worker(
            &format!("worker-{}", Uuid::new_v4()),
            &labels,
            StdDuration::from_secs(60),
        )
        .await
        .unwrap()
        .unwrap()
}
