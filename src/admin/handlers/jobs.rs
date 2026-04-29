use std::str::FromStr;

use anyhow::anyhow;
use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::admin::handlers::ApiResult;
use crate::admin::state::AppState;
use crate::domain::types::TaskType;
use crate::store::{CreateJob, JobRepository};

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub name: String,
    pub task_type: TaskType,
    pub config_json: Value,
    pub cron_expr: String,
    pub label_selector: String,
    pub timezone: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: Uuid,
    pub name: String,
    pub task_type: TaskType,
    pub enabled: bool,
    pub paused: bool,
    pub cron_expr: String,
    pub timezone: String,
    pub next_fire_at: String,
    pub label_selector: String,
}

pub async fn list_jobs(State(state): State<AppState>) -> ApiResult<Json<Vec<JobResponse>>> {
    let Some(api) = state.api else {
        return Ok(Json(Vec::new()));
    };

    let jobs = api.store.list_jobs().await?;
    Ok(Json(jobs.into_iter().map(JobResponse::from).collect()))
}

pub async fn create_job(
    State(state): State<AppState>,
    Json(request): Json<CreateJobRequest>,
) -> Result<Json<JobResponse>, StatusCode> {
    let Some(api) = state.api else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    let next_fire_at = match compute_next_fire_at(&request.cron_expr, request.timezone.as_deref()) {
        Ok(next_fire_at) => next_fire_at,
        Err(err) => {
            log::warn!("invalid job schedule: {err:#}");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let job = api
        .store
        .create(CreateJob {
            name: request.name,
            task_type: request.task_type,
            config_json: request.config_json,
            cron_expr: request.cron_expr,
            next_fire_at,
            label_selector: request.label_selector,
        })
        .await
        .map_err(|err| {
            log::error!("failed to create job: {err:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(JobResponse::from(job)))
}

fn compute_next_fire_at(cron_expr: &str, timezone: Option<&str>) -> anyhow::Result<DateTime<Utc>> {
    let timezone: chrono_tz::Tz = timezone.unwrap_or("Asia/Shanghai").parse()?;
    let schedule = cron::Schedule::from_str(cron_expr)?;
    let next = schedule
        .upcoming(timezone)
        .next()
        .ok_or_else(|| anyhow!("cron expression does not produce a next fire time"))?;
    Ok(next.with_timezone(&Utc))
}

impl From<crate::domain::types::Job> for JobResponse {
    fn from(job: crate::domain::types::Job) -> Self {
        Self {
            id: job.id,
            name: job.name,
            task_type: job.task_type,
            enabled: job.enabled,
            paused: job.paused,
            cron_expr: job.cron_expr,
            timezone: job.timezone,
            next_fire_at: job.next_fire_at.to_rfc3339(),
            label_selector: job.label_selector,
        }
    }
}
