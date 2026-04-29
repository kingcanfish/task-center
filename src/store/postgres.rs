use super::*;
use crate::domain::types::{ConcurrencyPolicy, MisfirePolicy, RouteStrategy};
use anyhow::{Result, anyhow};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        let jobs = sqlx::query_as::<_, Job>("SELECT * FROM jobs ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(jobs)
    }

    pub async fn list_executions(&self, limit: i64) -> Result<Vec<Execution>> {
        let executions = sqlx::query_as::<_, Execution>(
            "SELECT * FROM executions ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(executions)
    }

    pub async fn list_executions_for_job(&self, job_id: Uuid) -> Result<Vec<Execution>> {
        let executions = sqlx::query_as::<_, Execution>(
            "SELECT * FROM executions WHERE job_id = $1 ORDER BY created_at ASC",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(executions)
    }
}

#[async_trait]
impl JobRepository for PostgresStore {
    async fn list_enabled_due(&self, now: DateTime<Utc>) -> Result<Vec<Job>> {
        let jobs = sqlx::query_as::<_, Job>(
            "SELECT * FROM jobs WHERE enabled = true AND paused = false AND next_fire_at <= $1 ORDER BY next_fire_at ASC",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(jobs)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(job)
    }

    async fn create(&self, job: CreateJob) -> Result<Job> {
        let created = sqlx::query_as::<_, Job>(
            r#"
            INSERT INTO jobs (
                name, task_type, config_json, cron_expr, next_fire_at, label_selector,
                misfire_policy, route_strategy, concurrency_policy
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(job.name)
        .bind(job.task_type)
        .bind(job.config_json)
        .bind(job.cron_expr)
        .bind(job.next_fire_at)
        .bind(job.label_selector)
        .bind(MisfirePolicy::FireOnceNow)
        .bind(RouteStrategy::Random)
        .bind(ConcurrencyPolicy::AllowParallel)
        .fetch_one(&self.pool)
        .await?;
        Ok(created)
    }

    async fn update_next_fire_at(&self, id: Uuid, next_fire_at: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE jobs SET next_fire_at = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(next_fire_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_paused(&self, id: Uuid, paused: bool) -> Result<()> {
        sqlx::query("UPDATE jobs SET paused = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(paused)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl ExecutionRepository for PostgresStore {
    async fn create_execution(&self, input: CreateExecution) -> Result<Execution> {
        let execution = sqlx::query_as::<_, Execution>(
            r#"
            INSERT INTO executions (
                job_id, scheduled_at, manual_trigger_id, status, idempotency_key,
                shard_index, shard_total
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(input.job_id)
        .bind(input.scheduled_at)
        .bind(input.manual_trigger_id)
        .bind(ExecutionStatus::Scheduled)
        .bind(input.idempotency_key)
        .bind(input.shard_index)
        .bind(input.shard_total)
        .fetch_one(&self.pool)
        .await?;
        Ok(execution)
    }

    async fn create_or_get_execution(&self, input: CreateExecution) -> Result<Execution> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query_as::<_, Execution>(
            r#"
            INSERT INTO executions (
                job_id, scheduled_at, manual_trigger_id, status, idempotency_key,
                shard_index, shard_total
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (idempotency_key) DO NOTHING
            RETURNING *
            "#,
        )
        .bind(input.job_id)
        .bind(input.scheduled_at)
        .bind(input.manual_trigger_id)
        .bind(ExecutionStatus::Scheduled)
        .bind(&input.idempotency_key)
        .bind(input.shard_index)
        .bind(input.shard_total)
        .fetch_optional(&mut *tx)
        .await?;

        let execution = match inserted {
            Some(execution) => execution,
            None => {
                sqlx::query_as::<_, Execution>(
                    "SELECT * FROM executions WHERE idempotency_key = $1",
                )
                .bind(&input.idempotency_key)
                .fetch_one(&mut *tx)
                .await?
            }
        };

        tx.commit().await?;
        Ok(execution)
    }

    async fn get_execution(&self, id: Uuid) -> Result<Option<Execution>> {
        let execution = sqlx::query_as::<_, Execution>("SELECT * FROM executions WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(execution)
    }

    async fn update_status(&self, id: Uuid, status: ExecutionStatus) -> Result<()> {
        let result =
            sqlx::query("UPDATE executions SET status = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(status)
                .execute(&self.pool)
                .await?;

        if result.rows_affected() != 1 {
            return Err(anyhow!("execution not found: {id}"));
        }

        Ok(())
    }

    async fn create_attempt(&self, input: CreateAttempt) -> Result<ExecutionAttempt> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(input.execution_id)
            .execute(&mut *tx)
            .await?;

        let attempt_no = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT GREATEST($2, COALESCE(MAX(attempt_no), 0) + 1)
            FROM execution_attempts
            WHERE execution_id = $1
            "#,
        )
        .bind(input.execution_id)
        .bind(input.attempt_no)
        .fetch_one(&mut *tx)
        .await?;

        let attempt = sqlx::query_as::<_, ExecutionAttempt>(
            r#"
            INSERT INTO execution_attempts (execution_id, attempt_no, worker_id, status)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(input.execution_id)
        .bind(attempt_no)
        .bind(&input.worker_id)
        .bind(ExecutionStatus::Running)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE executions
            SET attempt_count = GREATEST(attempt_count, $2),
                selected_worker_id = $3,
                status = $4,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(input.execution_id)
        .bind(attempt_no)
        .bind(input.worker_id)
        .bind(ExecutionStatus::Running)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(attempt)
    }

    async fn finish_attempt(&self, input: FinishAttempt) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let attempt_result = sqlx::query(
            r#"
            UPDATE execution_attempts
            SET finished_at = now(),
                exit_code = $3,
                status = $4,
                stdout_summary = $5,
                stderr_summary = $6,
                error_message = $7,
                duration_ms = $8
            WHERE execution_id = $1 AND attempt_no = $2
            "#,
        )
        .bind(input.execution_id)
        .bind(input.attempt_no)
        .bind(input.exit_code)
        .bind(input.status)
        .bind(input.stdout_summary)
        .bind(input.stderr_summary)
        .bind(input.error_message)
        .bind(input.duration_ms)
        .execute(&mut *tx)
        .await?;

        if attempt_result.rows_affected() != 1 {
            return Err(anyhow!(
                "execution attempt not found: execution_id={}, attempt_no={}",
                input.execution_id,
                input.attempt_no
            ));
        }

        let execution_result =
            sqlx::query("UPDATE executions SET status = $2, updated_at = now() WHERE id = $1")
                .bind(input.execution_id)
                .bind(input.status)
                .execute(&mut *tx)
                .await?;

        if execution_result.rows_affected() != 1 {
            return Err(anyhow!("execution not found: {}", input.execution_id));
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_retry_due(&self, now: DateTime<Utc>) -> Result<Vec<Execution>> {
        let executions = sqlx::query_as::<_, Execution>(
            r#"
            SELECT *
            FROM executions
            WHERE status = $1 AND next_retry_at IS NOT NULL AND next_retry_at <= $2
            ORDER BY next_retry_at ASC
            "#,
        )
        .bind(ExecutionStatus::RetryWait)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(executions)
    }
}

#[async_trait]
impl WorkerRepository for PostgresStore {
    async fn upsert_snapshot(&self, snapshot: UpsertWorkerSnapshot) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO workers (worker_id, labels, capacity, status_snapshot, last_seen_snapshot)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT (worker_id) DO UPDATE
            SET labels = EXCLUDED.labels,
                capacity = EXCLUDED.capacity,
                status_snapshot = EXCLUDED.status_snapshot,
                last_seen_snapshot = now(),
                updated_at = now()
            "#,
        )
        .bind(snapshot.worker_id)
        .bind(snapshot.labels)
        .bind(snapshot.capacity)
        .bind(snapshot.status_snapshot)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_snapshots(&self) -> Result<Vec<WorkerSnapshot>> {
        let snapshots = sqlx::query_as::<_, WorkerSnapshot>(
            "SELECT * FROM workers ORDER BY last_seen_snapshot DESC, worker_id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(snapshots)
    }
}
