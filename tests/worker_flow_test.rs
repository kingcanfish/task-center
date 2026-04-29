use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use job_scheduler::config::{AppConfig, WorkerConfig};
use job_scheduler::domain::state::ExecutionStatus;
use job_scheduler::domain::types::{
    ConcurrencyPolicy, Execution, ExecutionAttempt, Job, MisfirePolicy, RouteStrategy, TaskType,
    WorkerHeartbeat,
};
use job_scheduler::executors::{ExecutionContext, TaskExecutor, TaskOutput};
use job_scheduler::store::{
    CreateAttempt, CreateExecution, CreateJob, ExecutionRepository, FinishAttempt, JobRepository,
};
use job_scheduler::worker::{
    execute_claimed_job_once, execute_claimed_once, heartbeat_from_config, worker_has_capacity,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[test]
fn worker_heartbeat_contains_labels_and_capacity() {
    let heartbeat = WorkerHeartbeat {
        worker_id: "worker-a".to_string(),
        labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
        capacity: 4,
        active_count: 1,
    };
    assert_eq!(heartbeat.worker_id, "worker-a");
    assert_eq!(
        heartbeat.labels.get("executor").map(String::as_str),
        Some("http")
    );
    assert_eq!(heartbeat.capacity, 4);
    assert_eq!(heartbeat.active_count, 1);
}

#[test]
fn worker_heartbeat_from_config_uses_labels_capacity_and_active_count() {
    let config = WorkerConfig {
        app: AppConfig {
            database_url: "postgres://localhost/task_center".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            access_token: "token".to_string(),
        },
        worker_id: "worker-b".to_string(),
        labels: BTreeMap::from([
            ("executor".to_string(), "shell".to_string()),
            ("region".to_string(), "local".to_string()),
        ]),
        max_concurrency: 8,
        heartbeat_interval: Duration::from_secs(10),
        offline_after: Duration::from_secs(30),
        enable_shell_executor: true,
        shell_allowed_commands: vec!["echo".to_string()],
    };

    let heartbeat = heartbeat_from_config(&config, 3);

    assert_eq!(heartbeat.worker_id, "worker-b");
    assert_eq!(
        heartbeat.labels.get("executor").map(String::as_str),
        Some("shell")
    );
    assert_eq!(heartbeat.capacity, 8);
    assert_eq!(heartbeat.active_count, 3);
}

#[test]
fn worker_capacity_gate_allows_only_below_max_concurrency() {
    assert!(worker_has_capacity(3, 4));
    assert!(!worker_has_capacity(4, 4));
}

#[tokio::test]
async fn execute_claimed_job_once_runs_shell_job_and_records_result() {
    let job_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    let store = FakeWorkerStore::new(
        worker_test_job(
            job_id,
            TaskType::Shell,
            json!({
                "command": "echo",
                "args": ["worker-ok"]
            }),
        ),
        worker_test_execution(execution_id, job_id),
    );
    let config = WorkerConfig {
        app: AppConfig {
            database_url: "postgres://localhost/task_center".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            access_token: "token".to_string(),
        },
        worker_id: "worker-shell".to_string(),
        labels: BTreeMap::from([("executor".to_string(), "shell".to_string())]),
        max_concurrency: 1,
        heartbeat_interval: Duration::from_secs(10),
        offline_after: Duration::from_secs(30),
        enable_shell_executor: true,
        shell_allowed_commands: vec!["echo".to_string()],
    };

    execute_claimed_job_once(
        &store,
        &store,
        &config,
        job_scheduler::coordinator::Lease {
            execution_id,
            worker_id: config.worker_id.clone(),
            attempt_no: 1,
        },
        CancellationToken::new(),
    )
    .await
    .expect("claimed shell job should execute");

    let state = store.state.lock().expect("worker store state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.finish_attempts.len(), 1);
    let finish = &state.finish_attempts[0];
    assert_eq!(finish.status, ExecutionStatus::Succeeded);
    assert_eq!(finish.exit_code, Some(0));
    assert!(
        finish
            .stdout_summary
            .as_deref()
            .unwrap_or_default()
            .contains("worker-ok")
    );
}

#[tokio::test]
async fn task_output_captures_success_summary() {
    let output = job_scheduler::executors::TaskOutput {
        exit_code: Some(0),
        stdout_summary: Some("ok".to_string()),
        stderr_summary: None,
        error_message: None,
    };
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(output.stdout_summary.as_deref(), Some("ok"));
}

#[tokio::test]
async fn execute_claimed_once_success_output_finishes_succeeded() {
    let repo = FakeExecutionRepository::with_next_attempt_no(7);
    let executor = FakeTaskExecutor::new(Ok(TaskOutput {
        exit_code: Some(0),
        stdout_summary: Some("ok".to_string()),
        stderr_summary: None,
        error_message: None,
    }));
    let execution_id = Uuid::new_v4();

    execute_claimed_once(
        &repo,
        &executor,
        execution_id,
        1,
        "worker-a".to_string(),
        json!({"kind": "test"}),
        execution_context(execution_id),
    )
    .await
    .expect("success output should complete");

    let state = repo.state.lock().expect("repo state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.create_attempts[0].execution_id, execution_id);
    assert_eq!(state.create_attempts[0].attempt_no, 1);
    assert_eq!(state.create_attempts[0].worker_id, "worker-a");

    assert_eq!(state.finish_attempts.len(), 1);
    let finish = &state.finish_attempts[0];
    assert_eq!(finish.execution_id, execution_id);
    assert_eq!(finish.attempt_no, 7);
    assert_eq!(finish.status, ExecutionStatus::Succeeded);
    assert_eq!(finish.exit_code, Some(0));
    assert_eq!(finish.stdout_summary.as_deref(), Some("ok"));
    assert_eq!(finish.error_message, None);
}

#[tokio::test]
async fn execute_claimed_once_executor_error_finishes_failed_and_returns_error() {
    let repo = FakeExecutionRepository::default();
    let executor = FakeTaskExecutor::new(Err(anyhow!("boom")));
    let execution_id = Uuid::new_v4();

    let err = execute_claimed_once(
        &repo,
        &executor,
        execution_id,
        2,
        "worker-b".to_string(),
        json!({}),
        execution_context(execution_id),
    )
    .await
    .expect_err("executor error should be returned");

    assert_eq!(err.to_string(), "boom");
    let state = repo.state.lock().expect("repo state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.finish_attempts.len(), 1);
    let finish = &state.finish_attempts[0];
    assert_eq!(finish.status, ExecutionStatus::Failed);
    assert_eq!(finish.error_message.as_deref(), Some("boom"));
}

#[tokio::test]
async fn execute_claimed_once_task_error_message_output_finishes_failed() {
    let repo = FakeExecutionRepository::default();
    let executor = FakeTaskExecutor::new(Ok(TaskOutput {
        exit_code: Some(0),
        stdout_summary: Some("partial".to_string()),
        stderr_summary: Some("failed".to_string()),
        error_message: Some("task failed".to_string()),
    }));
    let execution_id = Uuid::new_v4();

    execute_claimed_once(
        &repo,
        &executor,
        execution_id,
        3,
        "worker-c".to_string(),
        json!({}),
        execution_context(execution_id),
    )
    .await
    .expect("failed task output should be recorded without failing the worker");

    let state = repo.state.lock().expect("repo state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.finish_attempts.len(), 1);
    let finish = &state.finish_attempts[0];
    assert_eq!(finish.status, ExecutionStatus::Failed);
    assert_eq!(finish.exit_code, Some(0));
    assert_eq!(finish.stdout_summary.as_deref(), Some("partial"));
    assert_eq!(finish.stderr_summary.as_deref(), Some("failed"));
    assert_eq!(finish.error_message.as_deref(), Some("task failed"));
}

#[tokio::test]
async fn execute_claimed_once_nonzero_exit_output_finishes_failed() {
    let repo = FakeExecutionRepository::default();
    let executor = FakeTaskExecutor::new(Ok(TaskOutput {
        exit_code: Some(9),
        stdout_summary: Some("partial".to_string()),
        stderr_summary: Some("failed".to_string()),
        error_message: None,
    }));
    let execution_id = Uuid::new_v4();

    execute_claimed_once(
        &repo,
        &executor,
        execution_id,
        4,
        "worker-d".to_string(),
        json!({}),
        execution_context(execution_id),
    )
    .await
    .expect("nonzero task output should be recorded without failing the worker");

    let state = repo.state.lock().expect("repo state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.finish_attempts.len(), 1);
    let finish = &state.finish_attempts[0];
    assert_eq!(finish.status, ExecutionStatus::Failed);
    assert_eq!(finish.exit_code, Some(9));
    assert_eq!(finish.stdout_summary.as_deref(), Some("partial"));
    assert_eq!(finish.stderr_summary.as_deref(), Some("failed"));
    assert_eq!(finish.error_message, None);
}

#[tokio::test]
async fn execute_claimed_once_executor_error_preserves_finish_failure_context() {
    let repo = FakeExecutionRepository::with_finish_error("database unavailable");
    let executor = FakeTaskExecutor::new(Err(anyhow!("boom")));
    let execution_id = Uuid::new_v4();

    let err = execute_claimed_once(
        &repo,
        &executor,
        execution_id,
        5,
        "worker-e".to_string(),
        json!({}),
        execution_context(execution_id),
    )
    .await
    .expect_err("finish failure should be returned with executor context");

    let message = err.to_string();
    assert!(message.contains("executor failed: boom"));
    assert!(message.contains("database unavailable"));
    let state = repo.state.lock().expect("repo state poisoned");
    assert_eq!(state.create_attempts.len(), 1);
    assert_eq!(state.finish_attempts.len(), 0);
}

#[derive(Default)]
struct FakeExecutionRepository {
    state: Mutex<FakeExecutionRepositoryState>,
    finish_error: Mutex<Option<String>>,
    next_attempt_no: Mutex<Option<i32>>,
}

#[derive(Default)]
struct FakeExecutionRepositoryState {
    create_attempts: Vec<CreateAttempt>,
    finish_attempts: Vec<FinishAttempt>,
}

impl FakeExecutionRepository {
    fn with_finish_error(message: &str) -> Self {
        Self {
            state: Mutex::new(FakeExecutionRepositoryState::default()),
            finish_error: Mutex::new(Some(message.to_string())),
            next_attempt_no: Mutex::new(None),
        }
    }

    fn with_next_attempt_no(attempt_no: i32) -> Self {
        Self {
            state: Mutex::new(FakeExecutionRepositoryState::default()),
            finish_error: Mutex::new(None),
            next_attempt_no: Mutex::new(Some(attempt_no)),
        }
    }
}

#[async_trait]
impl ExecutionRepository for FakeExecutionRepository {
    async fn create_execution(&self, _input: CreateExecution) -> Result<Execution> {
        unimplemented!("not needed by worker finish-path tests")
    }

    async fn create_or_get_execution(&self, _input: CreateExecution) -> Result<Execution> {
        unimplemented!("not needed by worker finish-path tests")
    }

    async fn get_execution(&self, _id: Uuid) -> Result<Option<Execution>> {
        unimplemented!("not needed by worker finish-path tests")
    }

    async fn update_status(&self, _id: Uuid, _status: ExecutionStatus) -> Result<()> {
        unimplemented!("not needed by worker finish-path tests")
    }

    async fn create_attempt(&self, input: CreateAttempt) -> Result<ExecutionAttempt> {
        let returned_attempt_no = self
            .next_attempt_no
            .lock()
            .expect("repo next attempt poisoned")
            .take()
            .unwrap_or(input.attempt_no);
        self.state
            .lock()
            .expect("repo state poisoned")
            .create_attempts
            .push(input.clone());
        Ok(ExecutionAttempt {
            id: Uuid::new_v4(),
            execution_id: input.execution_id,
            attempt_no: returned_attempt_no,
            worker_id: input.worker_id,
            started_at: Utc::now(),
            finished_at: None,
            exit_code: None,
            status: ExecutionStatus::Running,
            stdout_summary: None,
            stderr_summary: None,
            error_message: None,
            duration_ms: None,
        })
    }

    async fn finish_attempt(&self, input: FinishAttempt) -> Result<()> {
        if let Some(message) = self
            .finish_error
            .lock()
            .expect("repo finish error poisoned")
            .take()
        {
            return Err(anyhow!(message));
        }
        self.state
            .lock()
            .expect("repo state poisoned")
            .finish_attempts
            .push(input);
        Ok(())
    }

    async fn list_retry_due(&self, _now: chrono::DateTime<Utc>) -> Result<Vec<Execution>> {
        unimplemented!("not needed by worker finish-path tests")
    }
}

struct FakeTaskExecutor {
    result: Mutex<Option<Result<TaskOutput>>>,
}

impl FakeTaskExecutor {
    fn new(result: Result<TaskOutput>) -> Self {
        Self {
            result: Mutex::new(Some(result)),
        }
    }
}

#[async_trait]
impl TaskExecutor for FakeTaskExecutor {
    async fn execute(&self, _config: Value, _context: ExecutionContext) -> Result<TaskOutput> {
        self.result
            .lock()
            .expect("executor result poisoned")
            .take()
            .expect("executor called more than once")
    }
}

fn execution_context(execution_id: Uuid) -> ExecutionContext {
    ExecutionContext {
        execution_id,
        scheduled_at: Utc::now(),
        shard_index: 0,
        shard_total: 1,
        cancel: CancellationToken::new(),
    }
}

struct FakeWorkerStore {
    job: Job,
    execution: Execution,
    state: Mutex<FakeExecutionRepositoryState>,
}

impl FakeWorkerStore {
    fn new(job: Job, execution: Execution) -> Self {
        Self {
            job,
            execution,
            state: Mutex::new(FakeExecutionRepositoryState::default()),
        }
    }
}

#[async_trait]
impl JobRepository for FakeWorkerStore {
    async fn list_enabled_due(&self, _now: chrono::DateTime<Utc>) -> Result<Vec<Job>> {
        Ok(Vec::new())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Job>> {
        Ok((id == self.job.id).then(|| self.job.clone()))
    }

    async fn create(&self, _job: CreateJob) -> Result<Job> {
        unimplemented!("not needed by worker job execution tests")
    }

    async fn update_next_fire_at(
        &self,
        _id: Uuid,
        _next_fire_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        Ok(())
    }

    async fn set_paused(&self, _id: Uuid, _paused: bool) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ExecutionRepository for FakeWorkerStore {
    async fn create_execution(&self, _input: CreateExecution) -> Result<Execution> {
        unimplemented!("not needed by worker job execution tests")
    }

    async fn create_or_get_execution(&self, _input: CreateExecution) -> Result<Execution> {
        unimplemented!("not needed by worker job execution tests")
    }

    async fn get_execution(&self, id: Uuid) -> Result<Option<Execution>> {
        Ok((id == self.execution.id).then(|| self.execution.clone()))
    }

    async fn update_status(&self, _id: Uuid, _status: ExecutionStatus) -> Result<()> {
        Ok(())
    }

    async fn create_attempt(&self, input: CreateAttempt) -> Result<ExecutionAttempt> {
        self.state
            .lock()
            .expect("worker store state poisoned")
            .create_attempts
            .push(input.clone());
        Ok(ExecutionAttempt {
            id: Uuid::new_v4(),
            execution_id: input.execution_id,
            attempt_no: input.attempt_no,
            worker_id: input.worker_id,
            started_at: Utc::now(),
            finished_at: None,
            exit_code: None,
            status: ExecutionStatus::Running,
            stdout_summary: None,
            stderr_summary: None,
            error_message: None,
            duration_ms: None,
        })
    }

    async fn finish_attempt(&self, input: FinishAttempt) -> Result<()> {
        self.state
            .lock()
            .expect("worker store state poisoned")
            .finish_attempts
            .push(input);
        Ok(())
    }

    async fn list_retry_due(&self, _now: chrono::DateTime<Utc>) -> Result<Vec<Execution>> {
        Ok(Vec::new())
    }
}

fn worker_test_job(id: Uuid, task_type: TaskType, config_json: Value) -> Job {
    let now = Utc::now();
    Job {
        id,
        name: "worker-test-job".to_string(),
        description: None,
        enabled: true,
        paused: false,
        task_type,
        config_json,
        cron_expr: "0 0 * * * *".to_string(),
        timezone: "UTC".to_string(),
        next_fire_at: now,
        misfire_policy: MisfirePolicy::FireOnceNow,
        misfire_grace_seconds: 3600,
        max_retries: 0,
        retry_delay_seconds: 60,
        timeout_seconds: 300,
        label_selector: String::new(),
        route_strategy: RouteStrategy::Random,
        concurrency_policy: ConcurrencyPolicy::AllowParallel,
        created_at: now,
        updated_at: now,
    }
}

fn worker_test_execution(id: Uuid, job_id: Uuid) -> Execution {
    let now = Utc::now();
    Execution {
        id,
        job_id,
        scheduled_at: now,
        manual_trigger_id: None,
        status: ExecutionStatus::Scheduled,
        idempotency_key: format!("test-{id}"),
        attempt_count: 0,
        shard_index: 0,
        shard_total: 1,
        selected_worker_id: None,
        next_retry_at: None,
        created_at: now,
        updated_at: now,
    }
}
