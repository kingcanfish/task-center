use job_scheduler::config::{AppConfig, WorkerConfig};
use job_scheduler::domain::types::WorkerHeartbeat;
use job_scheduler::worker::{heartbeat_from_config, worker_has_capacity};
use std::collections::BTreeMap;
use std::time::Duration;

#[test]
fn worker_heartbeat_contains_labels_and_capacity() {
    let heartbeat = WorkerHeartbeat {
        worker_id: "worker-a".to_string(),
        labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
        capacity: 4,
        active_count: 1,
    };
    assert_eq!(heartbeat.worker_id, "worker-a");
    assert_eq!(
        heartbeat.labels.get("executor").map(String::as_str),
        Some("http")
    );
    assert_eq!(heartbeat.capacity, 4);
    assert_eq!(heartbeat.active_count, 1);
}

#[test]
fn worker_heartbeat_from_config_uses_labels_capacity_and_active_count() {
    let config = WorkerConfig {
        app: AppConfig {
            database_url: "postgres://localhost/task_center".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            access_token: "token".to_string(),
        },
        worker_id: "worker-b".to_string(),
        labels: BTreeMap::from([
            ("executor".to_string(), "shell".to_string()),
            ("region".to_string(), "local".to_string()),
        ]),
        max_concurrency: 8,
        heartbeat_interval: Duration::from_secs(10),
        offline_after: Duration::from_secs(30),
        enable_shell_executor: true,
        shell_allowed_commands: vec!["echo".to_string()],
    };

    let heartbeat = heartbeat_from_config(&config, 3);

    assert_eq!(heartbeat.worker_id, "worker-b");
    assert_eq!(
        heartbeat.labels.get("executor").map(String::as_str),
        Some("shell")
    );
    assert_eq!(heartbeat.capacity, 8);
    assert_eq!(heartbeat.active_count, 3);
}

#[test]
fn worker_capacity_gate_allows_only_below_max_concurrency() {
    assert!(worker_has_capacity(3, 4));
    assert!(!worker_has_capacity(4, 4));
}
