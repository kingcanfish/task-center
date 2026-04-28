mod common;

use chrono::Utc;
use job_scheduler::domain::state::ExecutionStatus;
use job_scheduler::domain::types::TaskType;
use job_scheduler::store::postgres::PostgresStore;
use job_scheduler::store::{
    CreateExecution, CreateJob, ExecutionRepository, FinishAttempt, JobRepository,
};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn creates_and_reads_job() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let job = create_test_job(&store).await;

    let loaded = store.get(job.id).await.unwrap().unwrap();
    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.label_selector, "executor=http");
}

#[tokio::test]
async fn duplicate_execution_idempotency_key_is_rejected() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let job = create_test_job(&store).await;
    let idempotency_key = format!("idem_{}", Uuid::new_v4());

    store
        .create_execution(CreateExecution {
            job_id: job.id,
            scheduled_at: Utc::now(),
            manual_trigger_id: None,
            idempotency_key: idempotency_key.clone(),
            shard_index: 0,
            shard_total: 1,
        })
        .await
        .unwrap();

    let duplicate = store
        .create_execution(CreateExecution {
            job_id: job.id,
            scheduled_at: Utc::now(),
            manual_trigger_id: None,
            idempotency_key,
            shard_index: 0,
            shard_total: 1,
        })
        .await;

    assert!(duplicate.is_err());
}

#[tokio::test]
async fn finish_missing_attempt_returns_error_without_updating_execution() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let job = create_test_job(&store).await;
    let execution = store
        .create_execution(CreateExecution {
            job_id: job.id,
            scheduled_at: Utc::now(),
            manual_trigger_id: None,
            idempotency_key: format!("idem_{}", Uuid::new_v4()),
            shard_index: 0,
            shard_total: 1,
        })
        .await
        .unwrap();

    let result = store
        .finish_attempt(FinishAttempt {
            execution_id: execution.id,
            attempt_no: 99,
            status: ExecutionStatus::Succeeded,
            exit_code: Some(0),
            stdout_summary: None,
            stderr_summary: None,
            error_message: None,
            duration_ms: Some(100),
        })
        .await;

    assert!(result.is_err());

    let loaded = store.get_execution(execution.id).await.unwrap().unwrap();
    assert_eq!(loaded.status, ExecutionStatus::Scheduled);
}

#[tokio::test]
async fn update_status_missing_execution_returns_error() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);

    let result = store
        .update_status(Uuid::new_v4(), ExecutionStatus::Queued)
        .await;

    assert!(result.is_err());
}

async fn create_test_job(store: &PostgresStore) -> job_scheduler::domain::types::Job {
    store
        .create(CreateJob {
            name: format!("http_job_{}", Uuid::new_v4()),
            task_type: TaskType::Http,
            config_json: json!({"url": "https://example.com"}),
            cron_expr: "0 0 8 * * *".to_string(),
            next_fire_at: Utc::now(),
            label_selector: "executor=http".to_string(),
        })
        .await
        .unwrap()
}
