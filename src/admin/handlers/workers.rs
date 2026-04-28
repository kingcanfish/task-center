use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub worker_id: String,
    pub online: bool,
}

pub async fn list_workers() -> Json<Vec<WorkerResponse>> {
    Json(Vec::new())
}
