use job_scheduler::coordinator::redis::RedisCoordinator;
use job_scheduler::coordinator::{Coordinator, DispatchQueue, QueueItem};
use job_scheduler::domain::types::WorkerHeartbeat;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

async fn redis() -> RedisCoordinator {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    RedisCoordinator::connect(&url).await.unwrap()
}

#[tokio::test]
async fn lock_only_succeeds_once() {
    let redis = redis().await;
    let key = format!("test:lock:{}", Uuid::new_v4());
    assert!(redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
    assert!(!redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
}

#[tokio::test]
async fn heartbeat_marks_worker_live() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    redis
        .heartbeat(
            WorkerHeartbeat {
                worker_id: worker_id.clone(),
                labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
                capacity: 4,
                active_count: 0,
            },
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert!(redis.is_worker_live(&worker_id).await.unwrap());
}

#[tokio::test]
async fn claim_returns_enqueued_item_once() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    let labels = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let execution_id = Uuid::new_v4();
    redis
        .enqueue(QueueItem {
            execution_id,
            job_id: Uuid::new_v4(),
            label_selector: "executor=http".to_string(),
            selected_worker_id: Some(worker_id.clone()),
        })
        .await
        .unwrap();

    let lease = redis
        .claim_for_worker(&worker_id, &labels, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.execution_id, execution_id);
    assert!(
        redis
            .claim_for_worker(&worker_id, &labels, Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
}
