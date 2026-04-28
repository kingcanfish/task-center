use crate::coordinator::redis::RedisCoordinator;
use crate::coordinator::{Coordinator, DispatchQueue, Lease};
use crate::domain::state::ExecutionStatus;
use crate::domain::types::WorkerHeartbeat;
use crate::executors::{ExecutionContext, TaskExecutor, TaskOutput};
use crate::store::{CreateAttempt, ExecutionRepository, FinishAttempt};
use anyhow::{Context, Result, anyhow};
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

const ERROR_BACKOFF: Duration = Duration::from_secs(1);

pub async fn run(config: crate::config::WorkerConfig) -> Result<()> {
    log::info!("worker runtime is starting");
    validate_redis_url(&config)?;

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let Some(mut coordinator) = connect_until_ready_or_shutdown(&config, &mut shutdown).await?
    else {
        log::info!("worker runtime is shutting down");
        return Ok(());
    };
    let mut runtime_state = WorkerRuntimeState::default();

    let mut heartbeat_interval = tokio::time::interval(config.heartbeat_interval);
    heartbeat_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                log::info!("worker runtime is shutting down");
                break;
            }
            _ = heartbeat_interval.tick() => {
                let tick = worker_tick(&coordinator, &config, &mut runtime_state);
                tokio::select! {
                    _ = &mut shutdown => {
                        log::info!("worker runtime is shutting down");
                        break;
                    }
                    result = tick => {
                        if let Err(err) = result {
                            let action = classify_tick_error(&err);
                            log_tick_error(&err, action);
                            if action == TickErrorAction::Fatal {
                                return Err(err);
                            }
                            if action == TickErrorAction::Reconnect {
                                match reconnect_or_shutdown(&config, &mut shutdown).await? {
                                    ReconnectOutcome::Connected(reconnected) => {
                                        coordinator = reconnected;
                                    }
                                    ReconnectOutcome::RetryLater => {}
                                    ReconnectOutcome::Shutdown => {
                                        log::info!("worker runtime is shutting down");
                                        break;
                                    }
                                }
                            }
                            if !backoff_or_shutdown(&mut shutdown).await {
                                log::info!("worker runtime is shutting down");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_redis_url(config: &crate::config::WorkerConfig) -> Result<()> {
    redis::Client::open(config.app.redis_url.as_str())?;
    Ok(())
}

async fn connect_until_ready_or_shutdown(
    config: &crate::config::WorkerConfig,
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> Result<Option<RedisCoordinator>> {
    loop {
        match reconnect_or_shutdown(config, shutdown).await? {
            ReconnectOutcome::Connected(coordinator) => return Ok(Some(coordinator)),
            ReconnectOutcome::Shutdown => return Ok(None),
            ReconnectOutcome::RetryLater => {
                if !backoff_or_shutdown(shutdown).await {
                    return Ok(None);
                }
            }
        }
    }
}

pub fn heartbeat_from_config(
    config: &crate::config::WorkerConfig,
    active_count: usize,
) -> WorkerHeartbeat {
    WorkerHeartbeat {
        worker_id: config.worker_id.clone(),
        labels: config.labels.clone(),
        capacity: config.max_concurrency,
        active_count,
    }
}

pub fn worker_has_capacity(active_count: usize, max_concurrency: usize) -> bool {
    active_count < max_concurrency
}

pub async fn execute_claimed_once<E, X>(
    executions: &E,
    executor: &X,
    execution_id: uuid::Uuid,
    attempt_no: i32,
    worker_id: String,
    config: serde_json::Value,
    context: ExecutionContext,
) -> anyhow::Result<()>
where
    E: ExecutionRepository,
    X: TaskExecutor,
{
    let attempt = executions
        .create_attempt(CreateAttempt {
            execution_id,
            attempt_no,
            worker_id,
        })
        .await?;
    let attempt_no = attempt.attempt_no;

    let output = match executor.execute(config, context).await {
        Ok(output) => output,
        Err(err) => {
            let executor_error = err.to_string();
            let finish_result = executions
                .finish_attempt(FinishAttempt {
                    execution_id,
                    attempt_no,
                    status: ExecutionStatus::Failed,
                    exit_code: None,
                    stdout_summary: None,
                    stderr_summary: None,
                    error_message: Some(executor_error.clone()),
                    duration_ms: None,
                })
                .await;
            if let Err(finish_err) = finish_result {
                return Err(anyhow!(
                    "executor failed: {executor_error}; additionally failed to mark attempt failed: {finish_err:#}"
                ));
            }
            return Err(err);
        }
    };

    let status = task_output_status(&output);

    executions
        .finish_attempt(FinishAttempt {
            execution_id,
            attempt_no,
            status,
            exit_code: output.exit_code,
            stdout_summary: output.stdout_summary,
            stderr_summary: output.stderr_summary,
            error_message: output.error_message,
            duration_ms: None,
        })
        .await
        .with_context(|| {
            format!("failed to finish execution attempt {attempt_no} for {execution_id}")
        })?;

    Ok(())
}

fn task_output_status(output: &TaskOutput) -> ExecutionStatus {
    if output.error_message.is_some() || output.exit_code.is_some_and(|exit_code| exit_code != 0) {
        ExecutionStatus::Failed
    } else {
        ExecutionStatus::Succeeded
    }
}

#[derive(Debug, Default)]
pub struct WorkerRuntimeState {
    active_execution_ids: BTreeSet<Uuid>,
}

impl WorkerRuntimeState {
    pub fn active_count(&self) -> usize {
        self.active_execution_ids.len()
    }

    pub fn record_claim(&mut self, lease: &Lease) -> bool {
        self.active_execution_ids.insert(lease.execution_id)
    }
}

async fn worker_tick<C>(
    coordinator: &C,
    config: &crate::config::WorkerConfig,
    state: &mut WorkerRuntimeState,
) -> Result<()>
where
    C: Coordinator + DispatchQueue,
{
    let heartbeat = heartbeat_from_config(config, state.active_count());
    coordinator
        .heartbeat(heartbeat, config.offline_after)
        .await?;

    if !worker_has_capacity(state.active_count(), config.max_concurrency) {
        return Ok(());
    }

    if let Some(lease) = coordinator
        .claim_for_worker(&config.worker_id, &config.labels, config.offline_after)
        .await?
    {
        state.record_claim(&lease);
        log::info!("claimed execution {}", lease.execution_id);
    }

    Ok(())
}

enum ReconnectOutcome {
    Connected(RedisCoordinator),
    RetryLater,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickErrorAction {
    Reconnect,
    RetryLater,
    Fatal,
}

fn classify_tick_error(error: &anyhow::Error) -> TickErrorAction {
    let Some(redis_error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<redis::RedisError>())
    else {
        return TickErrorAction::Fatal;
    };

    if redis_error.is_io_error()
        || redis_error.is_connection_refusal()
        || redis_error.is_connection_dropped()
        || redis_error.is_timeout()
        || redis_error.is_unrecoverable_error()
    {
        return TickErrorAction::Reconnect;
    }

    match redis_error.kind() {
        redis::ErrorKind::BusyLoadingError
        | redis::ErrorKind::TryAgain
        | redis::ErrorKind::ClusterDown
        | redis::ErrorKind::MasterDown
        | redis::ErrorKind::ReadOnly => TickErrorAction::RetryLater,
        _ => TickErrorAction::Fatal,
    }
}

fn log_tick_error(error: &anyhow::Error, action: TickErrorAction) {
    match action {
        TickErrorAction::Reconnect => {
            log::error!("worker tick failed with reconnectable error: {error:#}");
        }
        TickErrorAction::RetryLater => {
            log::warn!("worker tick failed with retryable error: {error:#}");
        }
        TickErrorAction::Fatal => {
            log::error!("worker tick failed with fatal error: {error:#}");
        }
    }
}

async fn reconnect_or_shutdown(
    config: &crate::config::WorkerConfig,
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> Result<ReconnectOutcome> {
    let reconnect = RedisCoordinator::connect(&config.app.redis_url);
    tokio::select! {
        _ = shutdown => Ok(ReconnectOutcome::Shutdown),
        result = reconnect => {
            match result {
                Ok(coordinator) => {
                    log::info!("worker redis coordinator reconnected");
                    Ok(ReconnectOutcome::Connected(coordinator))
                }
                Err(err) => {
                    log::error!("worker redis reconnect failed: {err:#}");
                    Ok(ReconnectOutcome::RetryLater)
                }
            }
        }
    }
}

async fn backoff_or_shutdown(
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> bool {
    let backoff = tokio::time::sleep(ERROR_BACKOFF);
    tokio::pin!(backoff);
    tokio::select! {
        _ = shutdown => false,
        _ = &mut backoff => true,
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, WorkerConfig};
    use anyhow::anyhow;
    use async_trait::async_trait;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeCoordinator {
        heartbeats: Mutex<Vec<WorkerHeartbeat>>,
        claims: Mutex<VecDeque<Option<Lease>>>,
        claim_calls: Mutex<usize>,
    }

    #[async_trait]
    impl Coordinator for FakeCoordinator {
        async fn try_lock(&self, _key: &str, _ttl: Duration) -> Result<bool> {
            Ok(false)
        }

        async fn heartbeat(&self, heartbeat: WorkerHeartbeat, _ttl: Duration) -> Result<()> {
            self.heartbeats.lock().unwrap().push(heartbeat);
            Ok(())
        }

        async fn is_worker_live(&self, _worker_id: &str) -> Result<bool> {
            Ok(false)
        }

        async fn request_cancel(&self, _execution_id: Uuid, _ttl: Duration) -> Result<()> {
            Ok(())
        }

        async fn is_cancel_requested(&self, _execution_id: Uuid) -> Result<bool> {
            Ok(false)
        }
    }

    #[async_trait]
    impl DispatchQueue for FakeCoordinator {
        async fn enqueue(&self, _item: crate::coordinator::QueueItem) -> Result<()> {
            Ok(())
        }

        async fn claim_for_worker(
            &self,
            _worker_id: &str,
            _labels: &BTreeMap<String, String>,
            _lease_ttl: Duration,
        ) -> Result<Option<Lease>> {
            *self.claim_calls.lock().unwrap() += 1;
            Ok(self.claims.lock().unwrap().pop_front().flatten())
        }

        async fn queue_depth(&self, _queue: &str) -> Result<usize> {
            Ok(0)
        }
    }

    fn test_config(max_concurrency: usize) -> WorkerConfig {
        WorkerConfig {
            app: AppConfig {
                database_url: "postgres://localhost/task_center".to_string(),
                redis_url: "redis://localhost:6379".to_string(),
                access_token: "token".to_string(),
            },
            worker_id: "worker-a".to_string(),
            labels: BTreeMap::new(),
            max_concurrency,
            heartbeat_interval: Duration::from_secs(10),
            offline_after: Duration::from_secs(30),
            enable_shell_executor: true,
            shell_allowed_commands: vec!["echo".to_string()],
        }
    }

    #[tokio::test]
    async fn worker_tick_records_claims_and_stops_at_capacity() {
        let coordinator = FakeCoordinator::default();
        coordinator.claims.lock().unwrap().push_back(Some(Lease {
            execution_id: Uuid::new_v4(),
            worker_id: "worker-a".to_string(),
            attempt_no: 1,
        }));
        let config = test_config(1);
        let mut state = WorkerRuntimeState::default();

        worker_tick(&coordinator, &config, &mut state)
            .await
            .unwrap();
        worker_tick(&coordinator, &config, &mut state)
            .await
            .unwrap();

        assert_eq!(state.active_count(), 1);
        assert_eq!(*coordinator.claim_calls.lock().unwrap(), 1);
        let heartbeats = coordinator.heartbeats.lock().unwrap();
        assert_eq!(heartbeats[0].active_count, 0);
        assert_eq!(heartbeats[1].active_count, 1);
    }

    #[test]
    fn classifies_redis_io_error_as_reconnectable() {
        let redis_error =
            redis::RedisError::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));
        let error = anyhow!(redis_error);

        assert_eq!(classify_tick_error(&error), TickErrorAction::Reconnect);
    }

    #[test]
    fn classifies_queue_contract_error_as_fatal() {
        let redis_error =
            redis::RedisError::from((redis::ErrorKind::ResponseError, "invalid queue item json"));
        let error = anyhow!(redis_error);

        assert_eq!(classify_tick_error(&error), TickErrorAction::Fatal);
    }
}
