use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct QueueResponse {
    pub name: String,
    pub depth: usize,
}

pub async fn list_queues() -> Json<Vec<QueueResponse>> {
    Json(Vec::new())
}
