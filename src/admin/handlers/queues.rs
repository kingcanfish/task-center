use axum::{Json, extract::State};
use serde::Serialize;

use crate::admin::handlers::ApiResult;
use crate::admin::state::AppState;
use crate::coordinator::DispatchQueue;

#[derive(Debug, Serialize)]
pub struct QueueResponse {
    pub name: String,
    pub depth: usize,
}

pub async fn list_queues(State(state): State<AppState>) -> ApiResult<Json<Vec<QueueResponse>>> {
    let Some(api) = state.api else {
        return Ok(Json(Vec::new()));
    };

    Ok(Json(vec![QueueResponse {
        name: "queue:shared".to_string(),
        depth: api.coordinator.queue_depth("queue:shared").await?,
    }]))
}
