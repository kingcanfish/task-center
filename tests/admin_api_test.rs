use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use job_scheduler::admin::state::{AdminApiState, AppState};
use job_scheduler::coordinator::Coordinator;
use job_scheduler::coordinator::redis::RedisCoordinator;
use job_scheduler::domain::types::WorkerHeartbeat;
use job_scheduler::store::postgres::PostgresStore;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tower::ServiceExt;

mod common;

#[tokio::test]
async fn health_requires_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_rejects_wrong_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_returns_ok_with_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn job_routes_require_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn resource_lists_return_empty_arrays_with_access_token() {
    for uri in [
        "/api/jobs",
        "/api/executions",
        "/api/workers",
        "/api/queues",
    ] {
        let app = job_scheduler::admin::routes::test_router("secret".to_string());
        let response = app
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, json!([]), "{uri}");
    }
}

#[tokio::test]
async fn create_job_returns_not_implemented_with_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/jobs")
                .header("authorization", "Bearer secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "daily-report",
                        "task_type": "http",
                        "config_json": {},
                        "cron_expr": "0 0 * * * *",
                        "label_selector": "executor=http"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn unknown_ui_path_serves_index() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/jobs")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_success());
}

#[tokio::test]
async fn unknown_api_path_requires_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_api_path_returns_not_found_with_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/missing")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stateful_admin_api_creates_and_lists_resources() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let redis = RedisCoordinator::connect("redis://127.0.0.1:6379")
        .await
        .unwrap();
    let app = job_scheduler::admin::routes::router_with_state(
        "secret".to_string(),
        AppState::with_api(AdminApiState::new(store, redis.clone())),
    );

    let name = format!("api_job_{}", uuid::Uuid::new_v4());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/jobs")
                .header("authorization", "Bearer secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": name,
                        "task_type": "http",
                        "config_json": {
                            "method": "GET",
                            "url": "https://example.com"
                        },
                        "cron_expr": "0 0 * * * *",
                        "label_selector": "executor=http"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let created_body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let created: Value = serde_json::from_slice(&created_body).unwrap();
    assert_eq!(created["name"], name);
    assert_eq!(created["task_type"], "http");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let jobs_body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let jobs: Value = serde_json::from_slice(&jobs_body).unwrap();
    assert!(
        jobs.as_array()
            .unwrap()
            .iter()
            .any(|job| job["name"] == name),
        "{jobs}"
    );
}

#[tokio::test]
async fn stateful_admin_api_lists_live_workers() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let redis = RedisCoordinator::connect("redis://127.0.0.1:6379")
        .await
        .unwrap();
    let worker_id = format!("api-worker-{}", uuid::Uuid::new_v4());
    redis
        .heartbeat(
            WorkerHeartbeat {
                worker_id: worker_id.clone(),
                labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
                capacity: 4,
                active_count: 1,
            },
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    let app = job_scheduler::admin::routes::router_with_state(
        "secret".to_string(),
        AppState::with_api(AdminApiState::new(store, redis)),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/workers")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let workers: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        workers
            .as_array()
            .unwrap()
            .iter()
            .any(|worker| worker["worker_id"] == worker_id && worker["online"] == true),
        "{workers}"
    );
}
