use axum::{Json, extract::State};
use serde::Serialize;
use uuid::Uuid;

use crate::admin::handlers::ApiResult;
use crate::admin::state::AppState;
use crate::domain::state::ExecutionStatus;

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: Uuid,
    pub job_id: Uuid,
    pub scheduled_at: String,
    pub status: ExecutionStatus,
    pub attempt_count: i32,
    pub selected_worker_id: Option<String>,
    pub created_at: String,
}

pub async fn list_executions(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ExecutionResponse>>> {
    let Some(api) = state.api else {
        return Ok(Json(Vec::new()));
    };

    let executions = api.store.list_executions(100).await?;
    Ok(Json(
        executions
            .into_iter()
            .map(ExecutionResponse::from)
            .collect(),
    ))
}

impl From<crate::domain::types::Execution> for ExecutionResponse {
    fn from(execution: crate::domain::types::Execution) -> Self {
        Self {
            id: execution.id,
            job_id: execution.job_id,
            scheduled_at: execution.scheduled_at.to_rfc3339(),
            status: execution.status,
            attempt_count: execution.attempt_count,
            selected_worker_id: execution.selected_worker_id,
            created_at: execution.created_at.to_rfc3339(),
        }
    }
}
