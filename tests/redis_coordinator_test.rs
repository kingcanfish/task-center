use job_scheduler::coordinator::redis::RedisCoordinator;
use job_scheduler::coordinator::{Coordinator, DispatchQueue, QueueItem};
use job_scheduler::domain::types::WorkerHeartbeat;
use redis::AsyncCommands;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

async fn redis() -> RedisCoordinator {
    RedisCoordinator::connect(&redis_url()).await.unwrap()
}

async fn redis_connection() -> redis::aio::ConnectionManager {
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
async fn lock_only_succeeds_once() {
    let redis = redis().await;
    let key = format!("test:lock:{}", Uuid::new_v4());
    assert!(redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
    assert!(!redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
    delete_keys(&[key]).await;
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
    delete_keys(&[format!("worker:{worker_id}")]).await;
}

#[tokio::test]
async fn claim_returns_enqueued_item_once() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    let labels = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let execution_id = Uuid::new_v4();
    let direct_queue = format!("queue:worker:{worker_id}");
    let lease_key = format!("lease:{execution_id}");
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
    delete_keys(&[direct_queue, lease_key]).await;
}

#[tokio::test]
async fn direct_queue_nonmatching_item_stays_direct() {
    let redis = redis().await;
    let worker_a = format!("worker-a-{}", Uuid::new_v4());
    let worker_b = format!("worker-b-{}", Uuid::new_v4());
    let test_label = Uuid::new_v4().to_string();
    let labels_without_http = BTreeMap::from([
        ("executor".to_string(), "shell".to_string()),
        ("test".to_string(), test_label.clone()),
    ]);
    let labels_with_http = BTreeMap::from([
        ("executor".to_string(), "http".to_string()),
        ("test".to_string(), test_label.clone()),
    ]);
    let direct_queue = format!("queue:worker:{worker_a}");
    let shared_depth_before = redis.queue_depth("queue:shared").await.unwrap();

    redis
        .enqueue(QueueItem {
            execution_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            label_selector: format!("executor=http,test={test_label}"),
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
    delete_keys(&[direct_queue]).await;
}

#[tokio::test]
async fn duplicate_execution_queue_entries_only_create_one_lease() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    let labels = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let execution_id = Uuid::new_v4();
    let direct_queue = format!("queue:worker:{worker_id}");
    let lease_key = format!("lease:{execution_id}");
    let item = QueueItem {
        execution_id,
        job_id: Uuid::new_v4(),
        label_selector: "executor=http".to_string(),
        selected_worker_id: Some(worker_id.clone()),
    };

    redis.enqueue(item.clone()).await.unwrap();
    redis.enqueue(item).await.unwrap();

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
    assert_eq!(redis.queue_depth(&direct_queue).await.unwrap(), 0);
    delete_keys(&[direct_queue, lease_key]).await;
}

#[tokio::test]
async fn malformed_queue_item_is_not_lost() {
    let redis = redis().await;
    let mut conn = redis_connection().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    let labels = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let direct_queue = format!("queue:worker:{worker_id}");
    let _: () = conn.rpush(&direct_queue, "{not-json").await.unwrap();

    assert!(
        redis
            .claim_for_worker(&worker_id, &labels, Duration::from_secs(60))
            .await
            .is_err()
    );
    assert_eq!(redis.queue_depth(&direct_queue).await.unwrap(), 1);
    delete_keys(&[direct_queue]).await;
}
