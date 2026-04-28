use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: String,
    pub status: String,
}

pub async fn list_executions() -> Json<Vec<ExecutionResponse>> {
    Json(Vec::new())
}
