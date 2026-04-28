mod common;

use chrono::Utc;
use job_scheduler::domain::types::TaskType;
use job_scheduler::store::postgres::PostgresStore;
use job_scheduler::store::{CreateJob, JobRepository};
use serde_json::json;

#[tokio::test]
async fn creates_and_reads_job() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let job = store
        .create(CreateJob {
            name: format!("http_job_{}", uuid::Uuid::new_v4()),
            task_type: TaskType::Http,
            config_json: json!({"url": "https://example.com"}),
            cron_expr: "0 0 8 * * *".to_string(),
            next_fire_at: Utc::now(),
            label_selector: "executor=http".to_string(),
        })
        .await
        .unwrap();

    let loaded = store.get(job.id).await.unwrap().unwrap();
    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.label_selector, "executor=http");
}
