use axum::{Json, Router, http::StatusCode, middleware, routing::get};
use serde_json::json;

use crate::admin::auth::{AuthState, require_token};
use crate::admin::handlers::{executions, jobs, queues, workers};
use crate::admin::state::AppState;

pub fn router(access_token: String) -> Router {
    router_with_state(access_token, AppState::empty())
}

pub fn router_with_state(access_token: String, app_state: AppState) -> Router {
    let auth_state = AuthState { access_token };
    let api_router = Router::new()
        .route("/health", get(health))
        .route("/jobs", get(jobs::list_jobs).post(jobs::create_job))
        .route("/executions", get(executions::list_executions))
        .route("/workers", get(workers::list_workers))
        .route("/queues", get(queues::list_queues))
        .fallback(api_not_found)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_token,
        ));

    Router::new()
        .nest("/api", api_router)
        .with_state(app_state)
        .merge(crate::admin::r#static::static_routes())
}

pub fn test_router(access_token: String) -> Router {
    router(access_token)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}
