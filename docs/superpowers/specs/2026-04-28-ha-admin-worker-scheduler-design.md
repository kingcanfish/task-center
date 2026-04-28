# HA Admin/Worker Scheduler Design

Date: 2026-04-28

## Goal

Convert the current single-process Rust cron scheduler into a multi-instance, high-availability task platform inspired by XXL-JOB's admin/executor split. The platform will have:

- `task-center admin` for job configuration, scheduling, monitoring, and the embedded admin SPA.
- `task-center worker` for heartbeats, task claiming, execution, and result reporting.
- PostgreSQL as the source of record.
- Redis as the coordination center for locks, heartbeats, ready queues, leases, cancel flags, and route state.

The first version targets the full platform shape: admin API, embedded SPA, worker coordination, PostgreSQL + Redis, HTTP/Shell/Builtin tasks, worker monitoring, route strategies, failure handling, manual controls, and execution history.

Reference: XXL-JOB official repository, <https://github.com/xuxueli/xxl-job>.

## Current Project Context

The repository currently contains a single Rust service:

- `src/main.rs` starts a local `tokio-cron-scheduler`.
- `src/scheduler.rs` registers in-process cron jobs and sends Telegram notifications.
- `src/jobs/mod.rs` defines a `Job` trait with `name`, `cron_expr`, `run`, and `from_env`.
- Existing jobs load runtime configuration from environment variables.
- There is no persistent job configuration, execution history, worker registry, distributed lock, admin API, or web UI.

The new design replaces the local scheduler as the primary runtime path. Existing built-in jobs are migrated to config-driven executors.

## Confirmed Decisions

- Admin HA mode: multiple admin instances participate in scheduling. Redis atomic locks prevent duplicate dispatch.
- Storage: PostgreSQL + Redis mixed mode.
- Execution semantics: at least once.
- Dispatch model: workers actively pull and claim tasks.
- Task model: HTTP/Webhook, Shell, and Builtin Rust jobs share one task model.
- Route strategies: XXL-JOB-style strategies, including random, round-robin, least-active, failover, busyover, consistent hash, and broadcast.
- Failure handling: retries, timeout termination, failure notification, manual rerun, skip, kill running, and failover.
- Security model: internal network trust plus one shared `ACCESS_TOKEN`.
- Admin UI: embedded SPA served by the Rust admin process.
- UI structure: balanced console with Overview, Jobs, Executions, Workers, Queues, Alerts, and Settings.
- Binary layout: one binary with subcommands, `task-center admin` and `task-center worker`.
- Worker matching: label selector model.
- Secrets: task parameters are stored as plain PostgreSQL JSON in the first version.
- Heartbeat defaults: worker heartbeat every 10 seconds; offline after 30 seconds without heartbeat.
- Execution retention: unlimited in the first version.
- Existing env job config: migrate to admin/PostgreSQL configuration.
- Worker concurrency: `WORKER_MAX_CONCURRENCY` plus job-level concurrency policy.
- Misfire: per-job complete misfire policy.
- Logs: store only execution summaries in PostgreSQL.
- Redis role: coordination center, not just cache.

## Architecture

### Admin

`task-center admin` is responsible for:

- Serving the admin HTTP API.
- Serving the embedded SPA static assets.
- Managing PostgreSQL job configuration and execution history.
- Scanning enabled cron jobs and applying misfire policies.
- Acquiring Redis schedule locks before creating execution instances.
- Applying route strategies and enqueueing work into Redis.
- Monitoring worker liveness, queue depth, leases, retries, and failures.
- Providing manual operations: run, rerun, skip, kill, pause, resume.
- Sending notifications through the notifier abstraction.

Every admin instance may scan schedules. For each job, the instance must acquire `lock:schedule:<job_id>` in Redis before calculating due executions. PostgreSQL uniqueness constraints provide a second layer of duplicate protection.

### Worker

`task-center worker` is responsible for:

- Registering and refreshing heartbeat state in Redis.
- Reporting labels, capacity, and active execution counts.
- Claiming executable work from Redis queues.
- Refreshing leases while tasks run.
- Executing HTTP, Shell, or Builtin Rust tasks.
- Persisting attempt starts, attempt finishes, and execution results to PostgreSQL.
- Polling cancellation flags and terminating tasks when possible.

Workers do not need inbound network reachability. They actively pull work from the coordinator.

### Storage Responsibilities

PostgreSQL is the source of record:

- Jobs and schedules.
- Executions and attempts.
- Worker metadata snapshots.
- Admin action audit records.
- UI query data.

Redis is the coordination center:

- Distributed schedule locks.
- Worker heartbeat TTL keys.
- Ready queues.
- Execution leases.
- Cancel flags.
- Retry queue timing state if needed for efficient polling.
- Route strategy temporary state, such as round-robin cursors and active counts.

The first implementation does not include PG-only fallback. Interfaces should still isolate storage and coordination concerns so later implementations can replace Redis coordination or add PG-only behavior.

## Data Model

### `jobs`

Stores task definitions.

Core fields:

- `id`
- `name`
- `description`
- `enabled`
- `paused`
- `task_type`: `http`, `shell`, or `builtin`
- `config_json`
- `cron_expr`
- `timezone`
- `next_fire_at`
- `misfire_policy`
- `misfire_grace_seconds`
- `retry_policy`
- `max_retries`
- `retry_delay_seconds`
- `timeout_seconds`
- `label_selector`
- `route_strategy`
- `concurrency_policy`
- `created_at`
- `updated_at`

`config_json` is plain JSON in the first version. This is an intentional tradeoff under the internal-network trust model.

### `executions`

Stores one planned or manual execution instance.

Core fields:

- `id`
- `job_id`
- `scheduled_at`
- `manual_trigger_id`
- `status`
- `idempotency_key`
- `attempt_count`
- `shard_index`
- `shard_total`
- `selected_worker_id`
- `next_retry_at`
- `created_at`
- `updated_at`

Use a unique key over execution identity, for example `(job_id, scheduled_at, shard_index, manual_trigger_id)`, to protect against duplicate execution creation during admin races.

### `execution_attempts`

Stores each concrete run attempt.

Core fields:

- `id`
- `execution_id`
- `attempt_no`
- `worker_id`
- `started_at`
- `finished_at`
- `exit_code`
- `status`
- `stdout_summary`
- `stderr_summary`
- `error_message`
- `duration_ms`

Only summaries are stored. Full logs are out of scope for the first version.

### `workers`

Stores worker metadata snapshots for UI and history. Live status comes from Redis TTL.

Core fields:

- `worker_id`
- `name`
- `labels`
- `capacity`
- `last_seen_snapshot`
- `status_snapshot`
- `created_at`
- `updated_at`

### `admin_actions`

Records operator actions.

Core fields:

- `id`
- `action_type`
- `target_type`
- `target_id`
- `request_json`
- `result_json`
- `created_at`

The first security model has no per-user identity. The audit record still preserves what happened.

## Redis Keys

Representative keys:

- `lock:schedule:<job_id>`: short-lived lock for admin schedule scanning.
- `worker:<worker_id>`: heartbeat key with 30-second TTL.
- `queue:worker:<worker_id>`: direct queue for selected-worker routes.
- `queue:bucket:<bucket>`: general queue for label/bucket-based claiming.
- `lease:<execution_id>`: owner worker, attempt number, and expiry.
- `cancel:<execution_id>`: cancellation request flag.
- `route:round_robin:<job_id>`: route cursor.
- `route:active:<worker_id>`: active count cache.
- `route:hash:<job_id>`: optional consistent hash ring cache.

Exact key names can change during implementation, but the responsibilities should remain stable.

## Execution State Machine

Main path:

```text
scheduled -> queued -> leased -> running -> succeeded
```

Failure and retry path:

```text
running -> failed -> retry_wait -> queued
running -> timed_out -> retry_wait -> queued
retry_wait -> failed
```

Manual control path:

```text
scheduled -> skipped
queued -> skipped
running -> cancel_requested -> canceled
running -> cancel_requested -> timed_out
```

Worker loss path:

```text
running -> failed(worker_lost) -> retry_wait -> queued
running -> failed(worker_lost)
```

State transitions must be validated in code so invalid updates are rejected or ignored idempotently.

## Scheduling and Misfire

Every admin instance scans enabled, unpaused jobs. For each job:

1. Try to acquire `lock:schedule:<job_id>` in Redis.
2. If lock acquisition fails, skip the job for this scan.
3. Compute due fire times from `cron_expr`, `timezone`, `next_fire_at`, and current time.
4. Apply `misfire_policy`.
5. Create executions in PostgreSQL with duplicate protection.
6. Enqueue created executions into Redis.
7. Advance `next_fire_at`.

Misfire policies:

- `ignore`: advance schedule without creating missed executions.
- `fire_once_now`: create one execution for the latest missed fire time.
- `catch_up_all`: create executions for every missed fire time.
- `catch_up_window`: create executions only inside `misfire_grace_seconds`.

Manual executions use `manual_trigger_id` and do not affect cron `next_fire_at`.

## Routing

Workers are selected through labels. A worker heartbeat includes labels such as:

- `executor=http`
- `executor=shell`
- `builtin=bugutv_checkin`
- `builtin=bugutv_headless_checkin`
- `region=cn`
- `trusted=true`

A job declares a `label_selector`. The scheduler and worker claim logic only consider workers matching that selector.

Route strategies:

- `random`: choose a live matching worker randomly.
- `round_robin`: choose a live matching worker using Redis cursor state.
- `least_active`: choose a live matching worker with the lowest active count.
- `failover`: choose the first live matching worker in deterministic order.
- `busyover`: skip workers that are at or above capacity.
- `consistent_hash`: map job id, shard key, or configured hash key to a live worker.
- `broadcast`: create one shard execution per matching worker, using `shard_index` and `shard_total`.

Because workers pull work, route decisions are expressed as:

- Direct worker queues for selected-worker routes.
- Shared bucket queues for routes that allow any matching worker to claim.

## Worker Claiming

Worker loop:

1. Refresh heartbeat every 10 seconds.
2. Report labels, capacity, and active count.
3. If active count is below `WORKER_MAX_CONCURRENCY`, try to claim work.
4. Prefer `queue:worker:<worker_id>`.
5. Then inspect shared queues compatible with the worker labels.
6. Atomically claim an execution and create `lease:<execution_id>`.
7. Mark execution `running` and create an attempt record.
8. Execute the task.
9. Persist result and release or finish the lease.

Claim must be atomic. Two workers must not receive the same execution from Redis.

## Concurrency

Worker-level:

- `WORKER_MAX_CONCURRENCY` limits total active executions per worker.

Job-level:

- `allow_parallel`: allow multiple active executions for the same job.
- `skip_if_running`: skip new due execution when a previous one is still running.
- `queue_if_running`: keep new execution queued until previous finishes.
- `fail_if_running`: mark new execution failed when a previous one is still running.

Shell tasks are also limited by the worker shell policy.

## Failure Recovery and Control

### Retries

Each job configures:

- `max_retries`
- `retry_delay_seconds`
- `timeout_seconds`
- `failover_enabled`

On failure:

1. Write an `execution_attempts` row.
2. Increment `executions.attempt_count`.
3. If retries remain, set `retry_wait` and `next_retry_at`.
4. Requeue when due.
5. If retries are exhausted, set `failed` and notify.

### Worker Loss

If a running execution's lease expires and the owning worker heartbeat key is absent:

- If failover is enabled and retries remain, create a new attempt and requeue.
- Otherwise mark execution failed with `worker_lost` or `lease_expired`.

### Timeout

Workers enforce task timeout locally:

- HTTP: cancel the request future.
- Shell: terminate the child process, then force kill if needed.
- Builtin Rust jobs: use cooperative cancellation through `CancellationToken`.

Non-cooperative built-in jobs can remain in `cancel_requested` or timeout-pending state until they return.

### Kill

Admin kill behavior:

1. Set execution status to `cancel_requested`.
2. Write `cancel:<execution_id>` to Redis.
3. Worker observes the flag and cancels if possible.
4. Worker writes `canceled`, `timed_out`, or failure result.

If the worker is already offline, normal lease-expiry recovery handles the execution.

### Manual Operations

- `run`: create a manual execution.
- `rerun`: clone relevant historical execution context into a new manual execution.
- `skip`: mark a pending execution skipped; queued Redis entries are skipped lazily.
- `pause`: stop future schedule scanning for a job.
- `resume`: re-enable schedule scanning.

## Task Executors

All executors share a `TaskExecutor` interface that receives task config, execution context, and cancellation token.

### HTTP/Webhook

Config:

- `method`
- `url`
- `headers`
- `query`
- `body`
- `expected_statuses`
- `timeout_seconds`

Default success is HTTP 2xx. Store status code, response summary, and error summary.

### Shell

Shell executor is disabled unless explicitly enabled with a worker setting such as `ENABLE_SHELL_EXECUTOR=true`.

Config:

- `command`
- `args`
- `working_dir`
- `env`
- `allowed_command_policy`
- `timeout_seconds`
- `stdout_limit_bytes`
- `stderr_limit_bytes`

Rules:

- Prefer command plus args over raw shell strings.
- Do not support interactive shell sessions.
- `working_dir` must be inside allowed directories.
- Do not inherit all worker environment variables by default.
- Store only stdout/stderr summaries.
- Run as the container user; system-level user switching is out of scope.

### Builtin Rust Jobs

Replace env-driven `Job::from_env()` with a config-driven registry:

- `BuiltinExecutorRegistry` maps builtin names to handlers.
- Each handler declares its name and expected config schema.
- Each handler runs with an execution context containing `execution_id`, `scheduled_at`, shard info, cancellation token, notifier, and logger.

Existing Bugutv jobs become builtin executors:

- `bugutv_checkin`
- `bugutv_headless_checkin`

Credentials and cron move to PostgreSQL job configuration. Environment variables are no longer the primary configuration path.

## API Design

All admin and worker endpoints require `ACCESS_TOKEN` in the first version.

Admin API:

- `GET /api/health`
- `GET /api/jobs`
- `POST /api/jobs`
- `GET /api/jobs/:id`
- `PATCH /api/jobs/:id`
- `DELETE /api/jobs/:id`
- `POST /api/jobs/:id/run`
- `POST /api/jobs/:id/pause`
- `POST /api/jobs/:id/resume`
- `GET /api/executions`
- `GET /api/executions/:id`
- `POST /api/executions/:id/rerun`
- `POST /api/executions/:id/skip`
- `POST /api/executions/:id/kill`
- `GET /api/workers`
- `GET /api/workers/:id`
- `GET /api/queues`
- `GET /api/settings`
- `PATCH /api/settings`

Worker protocol:

- `POST /api/worker/heartbeat`
- `POST /api/worker/claim`
- `POST /api/worker/executions/:id/start`
- `POST /api/worker/executions/:id/finish`

The worker protocol is HTTP-based for the first version. It can later be moved behind a typed client or alternate transport without changing executor behavior.

## Admin SPA

The admin UI is an embedded SPA built separately and served by `task-center admin`.

Pages:

- Overview: scheduler health, live workers, running count, failures, retry queue, stuck executions.
- Jobs: create and edit HTTP, Shell, and Builtin jobs; configure cron, misfire, retries, timeout, labels, routes, concurrency; run, pause, resume.
- Executions: filter by job, status, worker, and time; inspect attempts; rerun, skip, kill.
- Workers: show online/offline state, labels, capacity, active count, running executions, heartbeat age.
- Queues: show ready queue depth, leases, retries, stuck entries, route diagnostics.
- Alerts: show recent failures, retry exhaustion, worker loss, timeout alerts.
- Settings: show access token guidance, Telegram settings, Shell executor defaults, global scheduler defaults.

The UI should be a dense operations console. Tables, filters, and clear status indicators are more important than decorative presentation.

## Notifications

Keep Telegram support but move it behind `Notifier`.

Events:

- Job failure.
- Retry exhausted.
- Timeout.
- Worker lost.
- Manual kill result.

Future notifier types can be added without changing scheduler logic.

## Compatibility Interfaces

Primary traits or modules:

- `JobRepository`
- `ExecutionRepository`
- `WorkerRepository`
- `Coordinator`
- `DispatchQueue`
- `RouteStrategy`
- `TaskExecutor`
- `Notifier`

Default implementations:

- PostgreSQL repositories.
- Redis coordinator and dispatch queue.
- Builtin route strategies.
- HTTP, Shell, and Builtin executors.
- Telegram notifier.

The implementation should keep these interfaces small and focused. Avoid coupling task execution logic directly to PostgreSQL or Redis clients.

## Deployment

Use one binary:

```text
task-center admin
task-center worker
```

Docker Compose should include:

- PostgreSQL.
- Redis.
- One or more admin instances.
- One or more worker instances.

Representative environment variables:

- `DATABASE_URL`
- `REDIS_URL`
- `ACCESS_TOKEN`
- `ADMIN_BIND_ADDR`
- `WORKER_ID`
- `WORKER_LABELS`
- `WORKER_MAX_CONCURRENCY`
- `WORKER_HEARTBEAT_INTERVAL_SECONDS`
- `WORKER_OFFLINE_AFTER_SECONDS`
- `ENABLE_SHELL_EXECUTOR`
- `SHELL_ALLOWED_COMMANDS`
- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID`

## Migration Plan

1. Add storage, migration, and configuration foundations.
2. Add admin and worker subcommands.
3. Add PostgreSQL repositories and Redis coordinator.
4. Add execution state machine and scheduler scanning.
5. Add worker heartbeat, claim, lease, and result reporting.
6. Add HTTP, Shell, and Builtin executors.
7. Migrate existing Bugutv jobs into builtin executor handlers.
8. Add admin API.
9. Add embedded SPA.
10. Add Docker Compose services for PostgreSQL, Redis, admin, and worker.
11. Update README with HA deployment and task configuration instructions.

The old env-driven scheduler path should be removed or kept only as an explicit legacy mode if needed later. The primary path is admin/PostgreSQL configuration.

## Testing Strategy

Unit tests:

- Cron due-time calculation.
- Misfire policies.
- Execution state transitions.
- Label selector matching.
- Route strategy selection.
- Retry, timeout, failover, and cancellation decisions.
- Shell command policy validation.
- Builtin executor config parsing.

Integration tests:

- PostgreSQL migrations apply cleanly.
- Multiple admin instances scanning the same job do not create duplicate executions.
- Multiple workers claiming concurrently do not receive the same execution.
- Worker heartbeat TTL drives online/offline state.
- Lease expiry triggers retry or failure.
- Broadcast creates shard executions for matching workers.
- Manual run, rerun, skip, kill, pause, and resume APIs behave correctly.
- Shell executor enforces whitelist, timeout, and output truncation.

End-to-end smoke tests:

- Create and run an HTTP job successfully.
- Create and run a Shell job successfully with allowed command policy.
- Create and run a Builtin Bugutv dry-run or mock job through the full admin-to-worker path.
- Verify the SPA can list jobs, executions, workers, and queues.

## Risks and Boundaries

- At-least-once execution means duplicate execution can happen during crashes, timeouts, lease expiry, or network partitions.
- `idempotency_key` is provided for tracing and future business-level idempotency, but the scheduler does not guarantee exactly-once behavior.
- `ACCESS_TOKEN` and plain PostgreSQL JSON parameters are acceptable only for trusted internal deployment.
- Shell execution is inherently risky. It must be disabled by default and constrained by explicit worker settings.
- Full route strategy support is broad. The design includes the full set, but implementation should verify each strategy independently.
- Execution history is retained indefinitely in the first version. Large installations will need retention or archival later.
- Full logs are out of scope. Only summaries are stored.
