use crate::{
    coordinator::{Coordinator, DispatchQueue, QueueItem},
    domain::types::{Job, MisfirePolicy},
    store::{CreateExecution, ExecutionRepository, JobRepository},
};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use std::time::Duration as StdDuration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisfireMode {
    Ignore,
    FireOnceNow,
    CatchUpAll,
    CatchUpWindow,
}

impl From<MisfirePolicy> for MisfireMode {
    fn from(value: MisfirePolicy) -> Self {
        match value {
            MisfirePolicy::Ignore => Self::Ignore,
            MisfirePolicy::FireOnceNow => Self::FireOnceNow,
            MisfirePolicy::CatchUpAll => Self::CatchUpAll,
            MisfirePolicy::CatchUpWindow => Self::CatchUpWindow,
        }
    }
}

impl From<MisfireMode> for MisfirePolicy {
    fn from(value: MisfireMode) -> Self {
        match value {
            MisfireMode::Ignore => Self::Ignore,
            MisfireMode::FireOnceNow => Self::FireOnceNow,
            MisfireMode::CatchUpAll => Self::CatchUpAll,
            MisfireMode::CatchUpWindow => Self::CatchUpWindow,
        }
    }
}

pub fn apply_misfire(
    mode: MisfireMode,
    mut fire_times: Vec<DateTime<Utc>>,
    now: DateTime<Utc>,
    grace_seconds: i64,
) -> Vec<DateTime<Utc>> {
    fire_times.sort();
    let fire_times: Vec<_> = fire_times.into_iter().filter(|time| *time <= now).collect();

    match mode {
        MisfireMode::Ignore => Vec::new(),
        MisfireMode::FireOnceNow => fire_times.into_iter().last().into_iter().collect(),
        MisfireMode::CatchUpAll => fire_times,
        MisfireMode::CatchUpWindow => {
            let cutoff = now - Duration::seconds(grace_seconds);
            fire_times
                .into_iter()
                .filter(|time| *time >= cutoff)
                .collect()
        }
    }
}

pub struct SchedulerService<J, E, C, Q> {
    pub jobs: J,
    pub executions: E,
    pub coordinator: C,
    pub queue: Q,
}

#[derive(Clone)]
pub struct SchedulerTick<S, C, Q> {
    store: S,
    coordinator: C,
    queue: Q,
}

impl<S, C, Q> SchedulerTick<S, C, Q> {
    pub fn new(store: S, coordinator: C, queue: Q) -> Self {
        Self {
            store,
            coordinator,
            queue,
        }
    }
}

impl<S, C, Q> SchedulerTick<S, C, Q>
where
    S: JobRepository + ExecutionRepository,
    C: Coordinator,
    Q: DispatchQueue,
{
    pub async fn run_once(&self, now: DateTime<Utc>) -> Result<()> {
        let mut failures = Vec::new();
        for job in self.store.list_enabled_due(now).await? {
            if let Err(err) = self.schedule_job(&job, now).await {
                log::error!("scheduler failed to process job {}: {err:#}", job.id);
                failures.push(format!("{}: {err:#}", job.id));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "scheduler tick failed for {} job(s): {}",
                failures.len(),
                failures.join("; ")
            ))
        }
    }

    async fn schedule_job(&self, job: &Job, now: DateTime<Utc>) -> Result<()> {
        let lock_key = format!("lock:schedule:{}", job.id);
        if !self
            .coordinator
            .try_lock(&lock_key, StdDuration::from_secs(30))
            .await?
        {
            return Ok(());
        }

        let execution = self
            .store
            .create_or_get_execution(CreateExecution {
                job_id: job.id,
                scheduled_at: job.next_fire_at,
                manual_trigger_id: None,
                idempotency_key: execution_idempotency_key(job),
                shard_index: 0,
                shard_total: 1,
            })
            .await?;

        self.queue
            .enqueue(QueueItem {
                execution_id: execution.id,
                job_id: job.id,
                label_selector: job.label_selector.clone(),
                selected_worker_id: None,
            })
            .await?;

        self.store
            .update_next_fire_at(job.id, now + Duration::minutes(1))
            .await?;
        Ok(())
    }
}

fn execution_idempotency_key(job: &Job) -> String {
    format!("{}:{}:0", job.id, job.next_fire_at.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::{Lease, QueueItem};
    use crate::domain::state::ExecutionStatus;
    use crate::domain::types::{
        ConcurrencyPolicy, Execution, ExecutionAttempt, RouteStrategy, TaskType, WorkerHeartbeat,
        WorkerSnapshot,
    };
    use crate::store::{
        CreateAttempt, CreateJob, FinishAttempt, UpsertWorkerSnapshot, WorkerRepository,
    };
    use anyhow::anyhow;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[test]
    fn ignore_misfire_returns_no_fire_times() {
        let previous = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let due = apply_misfire(MisfireMode::Ignore, vec![previous], now, 3600);
        assert!(due.is_empty());
    }

    #[test]
    fn fire_once_now_returns_latest_fire_time() {
        let first = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let due = apply_misfire(MisfireMode::FireOnceNow, vec![first, second], now, 3600);
        assert_eq!(due, vec![second]);
    }

    #[test]
    fn out_of_order_fire_times_are_sorted() {
        let first = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();

        let due = apply_misfire(MisfireMode::CatchUpAll, vec![second, first], now, 3600);

        assert_eq!(due, vec![first, second]);
    }

    #[test]
    fn future_fire_times_are_ignored() {
        let previous = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(2026, 4, 28, 11, 0, 0).unwrap();

        let due = apply_misfire(MisfireMode::CatchUpAll, vec![future, previous], now, 3600);

        assert_eq!(due, vec![previous]);
    }

    #[test]
    fn catch_up_window_includes_cutoff_boundary_and_excludes_older_times() {
        let older = Utc.with_ymd_and_hms(2026, 4, 28, 8, 59, 59).unwrap();
        let cutoff = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let inside = Utc.with_ymd_and_hms(2026, 4, 28, 9, 30, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();

        let due = apply_misfire(
            MisfireMode::CatchUpWindow,
            vec![inside, older, cutoff],
            now,
            3600,
        );

        assert_eq!(due, vec![cutoff, inside]);
    }

    #[test]
    fn misfire_policy_converts_to_and_from_misfire_mode() {
        assert_eq!(
            MisfireMode::from(MisfirePolicy::CatchUpWindow),
            MisfireMode::CatchUpWindow
        );
        assert_eq!(
            MisfirePolicy::from(MisfireMode::FireOnceNow),
            MisfirePolicy::FireOnceNow
        );
    }

    #[tokio::test]
    async fn scheduler_tick_continues_after_one_job_fails() {
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let first = fake_job("first", now - Duration::minutes(2));
        let second = fake_job("second", now - Duration::minutes(1));
        let updated_jobs = Arc::new(Mutex::new(Vec::new()));
        let store = FakeStore {
            jobs: vec![first.clone(), second.clone()],
            updated_jobs: updated_jobs.clone(),
        };
        let tick = SchedulerTick::new(
            store,
            FakeCoordinator,
            FakeQueue {
                fail_job_id: first.id,
            },
        );

        let err = tick.run_once(now).await.unwrap_err();

        assert!(err.to_string().contains("scheduler tick failed"));
        assert_eq!(*updated_jobs.lock().unwrap(), vec![second.id]);
    }

    #[derive(Clone)]
    struct FakeStore {
        jobs: Vec<Job>,
        updated_jobs: Arc<Mutex<Vec<Uuid>>>,
    }

    #[async_trait]
    impl JobRepository for FakeStore {
        async fn list_enabled_due(&self, _now: DateTime<Utc>) -> Result<Vec<Job>> {
            Ok(self.jobs.clone())
        }

        async fn get(&self, id: Uuid) -> Result<Option<Job>> {
            Ok(self.jobs.iter().find(|job| job.id == id).cloned())
        }

        async fn create(&self, _job: CreateJob) -> Result<Job> {
            Err(anyhow!("not implemented"))
        }

        async fn update_next_fire_at(&self, id: Uuid, _next_fire_at: DateTime<Utc>) -> Result<()> {
            self.updated_jobs.lock().unwrap().push(id);
            Ok(())
        }

        async fn set_paused(&self, _id: Uuid, _paused: bool) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ExecutionRepository for FakeStore {
        async fn create_execution(&self, input: CreateExecution) -> Result<Execution> {
            Ok(fake_execution(input))
        }

        async fn create_or_get_execution(&self, input: CreateExecution) -> Result<Execution> {
            Ok(fake_execution(input))
        }

        async fn get_execution(&self, _id: Uuid) -> Result<Option<Execution>> {
            Ok(None)
        }

        async fn update_status(&self, _id: Uuid, _status: ExecutionStatus) -> Result<()> {
            Ok(())
        }

        async fn create_attempt(&self, _input: CreateAttempt) -> Result<ExecutionAttempt> {
            Err(anyhow!("not implemented"))
        }

        async fn finish_attempt(&self, _input: FinishAttempt) -> Result<()> {
            Ok(())
        }

        async fn list_retry_due(&self, _now: DateTime<Utc>) -> Result<Vec<Execution>> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl WorkerRepository for FakeStore {
        async fn upsert_snapshot(&self, _snapshot: UpsertWorkerSnapshot) -> Result<()> {
            Ok(())
        }

        async fn list_snapshots(&self) -> Result<Vec<WorkerSnapshot>> {
            Ok(Vec::new())
        }
    }

    struct FakeCoordinator;

    #[async_trait]
    impl Coordinator for FakeCoordinator {
        async fn try_lock(&self, _key: &str, _ttl: std::time::Duration) -> Result<bool> {
            Ok(true)
        }

        async fn heartbeat(
            &self,
            _heartbeat: WorkerHeartbeat,
            _ttl: std::time::Duration,
        ) -> Result<()> {
            Ok(())
        }

        async fn is_worker_live(&self, _worker_id: &str) -> Result<bool> {
            Ok(false)
        }

        async fn request_cancel(
            &self,
            _execution_id: Uuid,
            _ttl: std::time::Duration,
        ) -> Result<()> {
            Ok(())
        }

        async fn is_cancel_requested(&self, _execution_id: Uuid) -> Result<bool> {
            Ok(false)
        }
    }

    struct FakeQueue {
        fail_job_id: Uuid,
    }

    #[async_trait]
    impl DispatchQueue for FakeQueue {
        async fn enqueue(&self, item: QueueItem) -> Result<()> {
            if item.job_id == self.fail_job_id {
                Err(anyhow!("queue unavailable"))
            } else {
                Ok(())
            }
        }

        async fn claim_for_worker(
            &self,
            _worker_id: &str,
            _labels: &std::collections::BTreeMap<String, String>,
            _lease_ttl: std::time::Duration,
        ) -> Result<Option<Lease>> {
            Ok(None)
        }

        async fn queue_depth(&self, _queue: &str) -> Result<usize> {
            Ok(0)
        }
    }

    fn fake_job(name: &str, next_fire_at: DateTime<Utc>) -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            enabled: true,
            paused: false,
            task_type: TaskType::Http,
            config_json: json!({"url": "https://example.com"}),
            cron_expr: "0 0 8 * * *".to_string(),
            timezone: "UTC".to_string(),
            next_fire_at,
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

    fn fake_execution(input: CreateExecution) -> Execution {
        let now = Utc::now();
        Execution {
            id: Uuid::new_v4(),
            job_id: input.job_id,
            scheduled_at: input.scheduled_at,
            manual_trigger_id: input.manual_trigger_id,
            status: ExecutionStatus::Scheduled,
            idempotency_key: input.idempotency_key,
            attempt_count: 0,
            shard_index: input.shard_index,
            shard_total: input.shard_total,
            selected_worker_id: None,
            next_retry_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
