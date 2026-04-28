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

#[tokio::test]
async fn direct_queue_nonmatching_item_stays_direct() {
    let redis = redis().await;
    let worker_a = format!("worker-a-{}", Uuid::new_v4());
    let worker_b = format!("worker-b-{}", Uuid::new_v4());
    let labels_without_http = BTreeMap::from([("executor".to_string(), "shell".to_string())]);
    let labels_with_http = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let direct_queue = format!("queue:worker:{worker_a}");
    let shared_depth_before = redis.queue_depth("queue:shared").await.unwrap();

    redis
        .enqueue(QueueItem {
            execution_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            label_selector: "executor=http".to_string(),
            selected_worker_id: Some(worker_a.clone()),
        })
        .await
        .unwrap();

    assert!(
        redis
            .claim_for_worker(&worker_a, &labels_without_http, Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        redis.queue_depth("queue:shared").await.unwrap(),
        shared_depth_before
    );
    assert_eq!(redis.queue_depth(&direct_queue).await.unwrap(), 1);
    assert!(
        redis
            .claim_for_worker(&worker_b, &labels_with_http, Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(redis.queue_depth(&direct_queue).await.unwrap(), 1);
}
