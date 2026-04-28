use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

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
