use axum::{Json, extract::State};
use serde::Serialize;
use serde_json::Value;

use crate::admin::handlers::ApiResult;
use crate::admin::state::AppState;

#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub worker_id: String,
    pub online: bool,
    pub labels: Value,
    pub capacity: usize,
    pub active_count: usize,
}

pub async fn list_workers(State(state): State<AppState>) -> ApiResult<Json<Vec<WorkerResponse>>> {
    let Some(api) = state.api else {
        return Ok(Json(Vec::new()));
    };

    let heartbeats = api.coordinator.list_worker_heartbeats().await?;
    Ok(Json(
        heartbeats
            .into_iter()
            .map(|heartbeat| WorkerResponse {
                worker_id: heartbeat.worker_id,
                online: true,
                labels: serde_json::to_value(heartbeat.labels).unwrap_or(Value::Null),
                capacity: heartbeat.capacity,
                active_count: heartbeat.active_count,
            })
            .collect(),
    ))
}
