CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL UNIQUE,
    description text,
    enabled boolean NOT NULL DEFAULT true,
    paused boolean NOT NULL DEFAULT false,
    task_type text NOT NULL CHECK (task_type IN ('http', 'shell', 'builtin')),
    config_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    cron_expr text NOT NULL,
    timezone text NOT NULL DEFAULT 'Asia/Shanghai',
    next_fire_at timestamptz NOT NULL,
    misfire_policy text NOT NULL CHECK (misfire_policy IN ('ignore', 'fire_once_now', 'catch_up_all', 'catch_up_window')),
    misfire_grace_seconds bigint NOT NULL DEFAULT 3600,
    max_retries integer NOT NULL DEFAULT 0,
    retry_delay_seconds bigint NOT NULL DEFAULT 60,
    timeout_seconds bigint NOT NULL DEFAULT 300,
    label_selector text NOT NULL DEFAULT '',
    route_strategy text NOT NULL CHECK (route_strategy IN ('random', 'round_robin', 'least_active', 'failover', 'busyover', 'consistent_hash', 'broadcast')),
    concurrency_policy text NOT NULL CHECK (concurrency_policy IN ('allow_parallel', 'skip_if_running', 'queue_if_running', 'fail_if_running')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE executions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    scheduled_at timestamptz NOT NULL,
    manual_trigger_id uuid,
    status text NOT NULL CHECK (status IN ('scheduled', 'queued', 'leased', 'running', 'succeeded', 'failed', 'retry_wait', 'timed_out', 'cancel_requested', 'canceled', 'skipped')),
    idempotency_key text NOT NULL UNIQUE,
    attempt_count integer NOT NULL DEFAULT 0,
    shard_index integer NOT NULL DEFAULT 0,
    shard_total integer NOT NULL DEFAULT 1,
    selected_worker_id text,
    next_retry_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (job_id, scheduled_at, shard_index, manual_trigger_id)
);

CREATE INDEX executions_job_status_idx ON executions(job_id, status);
CREATE INDEX executions_status_retry_idx ON executions(status, next_retry_at);
CREATE INDEX executions_worker_idx ON executions(selected_worker_id);

CREATE TABLE execution_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    execution_id uuid NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    attempt_no integer NOT NULL,
    worker_id text NOT NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    exit_code integer,
    status text NOT NULL CHECK (status IN ('scheduled', 'queued', 'leased', 'running', 'succeeded', 'failed', 'retry_wait', 'timed_out', 'cancel_requested', 'canceled', 'skipped')),
    stdout_summary text,
    stderr_summary text,
    error_message text,
    duration_ms bigint,
    UNIQUE (execution_id, attempt_no)
);

CREATE TABLE workers (
    worker_id text PRIMARY KEY,
    name text,
    labels jsonb NOT NULL DEFAULT '{}'::jsonb,
    capacity integer NOT NULL DEFAULT 1,
    last_seen_snapshot timestamptz NOT NULL DEFAULT now(),
    status_snapshot jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE admin_actions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    action_type text NOT NULL,
    target_type text NOT NULL,
    target_id text NOT NULL,
    request_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    result_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
