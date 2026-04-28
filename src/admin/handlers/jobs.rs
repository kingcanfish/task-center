use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub name: String,
    pub task_type: String,
    pub config_json: Value,
    pub cron_expr: String,
    pub label_selector: String,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub enabled: bool,
    pub paused: bool,
}

pub async fn list_jobs() -> Json<Vec<JobResponse>> {
    Json(Vec::new())
}

pub async fn create_job(
    Json(_request): Json<CreateJobRequest>,
) -> Result<Json<JobResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
