# HA Admin/Worker Scheduler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a high-availability admin/worker scheduler with PostgreSQL as source of record, Redis as coordinator, worker pull execution, admin API, embedded SPA, and HTTP/Shell/Builtin task executors.

**Architecture:** Keep one Rust binary with `task-center admin` and `task-center worker` subcommands. Store durable state in PostgreSQL, keep locks/heartbeats/queues/leases/cancel flags in Redis, and isolate both behind small repository and coordinator traits. Implement the platform in vertical slices so each commit compiles and has focused tests.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx/PostgreSQL, Redis, Serde, Chrono/chrono-tz, Clap, Tower HTTP, Reqwest, React/Vite/TypeScript for embedded admin UI.

---

## Scope Check

The approved spec covers several subsystems. This plan keeps one end-to-end implementation track but splits it into separately testable commits:

1. CLI/config/domain foundation.
2. PostgreSQL persistence.
3. Redis coordinator and queue.
4. Scheduler and route strategies.
5. Worker protocol and executors.
6. Admin HTTP API.
7. Embedded SPA.
8. Deployment, docs, and smoke verification.

Do not implement unrelated authentication, secret encryption, full log storage, PG-only fallback, or role-based access control. The first version uses `ACCESS_TOKEN`, plain `config_json`, at-least-once execution, and summary logs.

## Current Worktree Note

Before editing, run:

```bash
git status --short
```

Expected: the repository may already contain user changes in `.env.example`, `Cargo.toml`, `README.md`, `src/jobs/bugutv.rs`, `src/jobs/mod.rs`, `src/main.rs`, `.codex`, and `src/jobs/bugutv_headless.rs`. Preserve those changes. Only stage files owned by the task being committed.

## Target File Structure

Create and modify these paths:

- Modify `Cargo.toml`: add CLI, HTTP server, PostgreSQL, Redis, UUID, time, and frontend embedding dependencies.
- Create `src/lib.rs`: library exports used by integration tests and the binary.
- Modify `src/main.rs`: parse subcommands and dispatch admin/worker.
- Create `src/config.rs`: environment-backed config for admin, worker, database, Redis, auth, shell policy, and notification.
- Create `src/domain/mod.rs`: domain module exports.
- Create `src/domain/types.rs`: job, execution, attempt, worker, policy, and API DTO types.
- Create `src/domain/state.rs`: execution state machine and transition validation.
- Create `src/domain/labels.rs`: label map and selector parser/matcher.
- Create `src/store/mod.rs`: repository traits.
- Create `src/store/postgres.rs`: SQLx-backed repositories.
- Create `db/migrations/000001_ha_scheduler.up.sql`: schema.
- Create `db/migrations/000001_ha_scheduler.down.sql`: rollback.
- Create `src/coordinator/mod.rs`: coordinator and queue traits.
- Create `src/coordinator/redis.rs`: Redis locks, heartbeats, queue, leases, and cancel flags.
- Create `src/routing/mod.rs`: route strategy trait and implementations.
- Create `src/scheduling/mod.rs`: cron scanning, misfire handling, execution creation.
- Create `src/worker/mod.rs`: worker runtime loop.
- Create `src/worker/client.rs`: worker HTTP client for admin protocol if the worker reports through admin API.
- Create `src/executors/mod.rs`: task executor trait and registry.
- Create `src/executors/http.rs`: HTTP/Webhook executor.
- Create `src/executors/shell.rs`: Shell executor and command policy.
- Create `src/executors/builtin.rs`: built-in executor registry.
- Create `src/executors/bugutv.rs`: config-driven wrappers for existing Bugutv jobs.
- Modify `src/notify.rs`: move Telegram behind a `Notifier` trait.
- Create `src/admin/mod.rs`: admin server assembly.
- Create `src/admin/auth.rs`: `ACCESS_TOKEN` middleware/extractor.
- Create `src/admin/routes.rs`: Axum router.
- Create `src/admin/handlers/jobs.rs`: job endpoints.
- Create `src/admin/handlers/executions.rs`: execution endpoints.
- Create `src/admin/handlers/workers.rs`: worker endpoints.
- Create `src/admin/handlers/queues.rs`: queue endpoints.
- Create `src/admin/handlers/settings.rs`: settings endpoints.
- Create `src/admin/static.rs`: embedded SPA serving.
- Create `admin-ui/package.json`: SPA dependencies and scripts.
- Create `admin-ui/index.html`: Vite entry.
- Create `admin-ui/src/main.tsx`: React entry.
- Create `admin-ui/src/api.ts`: API client with token header.
- Create `admin-ui/src/App.tsx`: shell layout.
- Create `admin-ui/src/pages/Overview.tsx`: overview page.
- Create `admin-ui/src/pages/Jobs.tsx`: jobs page.
- Create `admin-ui/src/pages/Executions.tsx`: executions page.
- Create `admin-ui/src/pages/Workers.tsx`: workers page.
- Create `admin-ui/src/pages/Queues.tsx`: queues page.
- Create `admin-ui/src/pages/Settings.tsx`: settings page.
- Modify `docker-compose.yml`: PostgreSQL, Redis, admin, and worker services.
- Modify `Dockerfile`: build Rust binary and admin UI assets.
- Modify `.env.example`: HA runtime env vars.
- Modify `README.md`: admin/worker deployment and task configuration.
- Create `tests/common/mod.rs`: shared integration helpers.
- Create `tests/postgres_store_test.rs`: repository integration tests.
- Create `tests/redis_coordinator_test.rs`: Redis integration tests.
- Create `tests/admin_api_test.rs`: Axum API tests.
- Create `tests/worker_flow_test.rs`: worker claim/execution tests.

## Task 1: Dependencies, CLI, And Config Foundation

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Modify: `src/main.rs`
- Create: `src/config.rs`

- [ ] **Step 1: Add dependencies**

Edit `Cargo.toml` so dependencies include these crates in addition to existing job-specific crates:

```toml
clap = { version = "4", features = ["derive", "env"] }
axum = "0.7"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "fs", "trace"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid", "json", "migrate"] }
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
uuid = { version = "1", features = ["serde", "v4", "v7"] }
thiserror = "2"
cron = "0.12"
```

Run:

```bash
cargo check
```

Expected: dependencies resolve. Compilation may fail because new modules are not wired yet.

- [ ] **Step 2: Write CLI/config tests**

Create a `#[cfg(test)]` module at the bottom of `src/config.rs` with these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worker_labels() {
        let labels = parse_env_labels("executor=http,region=cn,trusted=true").unwrap();
        assert_eq!(labels.get("executor").map(String::as_str), Some("http"));
        assert_eq!(labels.get("region").map(String::as_str), Some("cn"));
        assert_eq!(labels.get("trusted").map(String::as_str), Some("true"));
    }

    #[test]
    fn rejects_empty_access_token() {
        let err = validate_access_token("").unwrap_err();
        assert!(err.to_string().contains("ACCESS_TOKEN"));
    }
}
```

Run:

```bash
cargo test config::tests -- --nocapture
```

Expected: fail because `parse_env_labels` and `validate_access_token` are not defined.

- [ ] **Step 3: Implement config module**

Create `src/config.rs` with:

```rust
use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub access_token: String,
}

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub app: AppConfig,
    pub bind_addr: String,
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub app: AppConfig,
    pub worker_id: String,
    pub labels: BTreeMap<String, String>,
    pub max_concurrency: usize,
    pub heartbeat_interval: Duration,
    pub offline_after: Duration,
    pub enable_shell_executor: bool,
    pub shell_allowed_commands: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = env_required("DATABASE_URL")?;
        let redis_url = env_required("REDIS_URL")?;
        let access_token = env_required("ACCESS_TOKEN")?;
        validate_access_token(&access_token)?;
        Ok(Self { database_url, redis_url, access_token })
    }
}

impl AdminConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            app: AppConfig::from_env()?,
            bind_addr: env::var("ADMIN_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        })
    }
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let app = AppConfig::from_env()?;
        let worker_id = env_required("WORKER_ID")?;
        let labels = parse_env_labels(&env::var("WORKER_LABELS").unwrap_or_default())?;
        let max_concurrency = env::var("WORKER_MAX_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4);
        let heartbeat_interval = env_seconds("WORKER_HEARTBEAT_INTERVAL_SECONDS", 10);
        let offline_after = env_seconds("WORKER_OFFLINE_AFTER_SECONDS", 30);
        let enable_shell_executor = env::var("ENABLE_SHELL_EXECUTOR")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let shell_allowed_commands = env::var("SHELL_ALLOWED_COMMANDS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|v| {
                let trimmed = v.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .collect();
        Ok(Self {
            app,
            worker_id,
            labels,
            max_concurrency,
            heartbeat_interval,
            offline_after,
            enable_shell_executor,
            shell_allowed_commands,
        })
    }
}

fn env_required(name: &str) -> Result<String> {
    let value = env::var(name).map_err(|_| anyhow!("{name} is required"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("{name} is required"));
    }
    Ok(value)
}

fn env_seconds(name: &str, default: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default))
}

pub fn validate_access_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        return Err(anyhow!("ACCESS_TOKEN is required"));
    }
    Ok(())
}

pub fn parse_env_labels(input: &str) -> Result<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    for item in input.split(',').map(str::trim).filter(|item| !item.is_empty()) {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid worker label `{item}`, expected key=value"))?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(anyhow!("invalid worker label `{item}`, expected non-empty key and value"));
        }
        labels.insert(key.to_string(), value.to_string());
    }
    Ok(labels)
}
```

- [ ] **Step 4: Add library exports and stub modules**

Create `src/lib.rs`:

```rust
pub mod admin;
pub mod config;
pub mod coordinator;
pub mod domain;
pub mod executors;
pub mod jobs;
pub mod notify;
pub mod routing;
pub mod scheduling;
pub mod scheduler;
pub mod store;
pub mod worker;
```

Create `src/admin/mod.rs`:

```rust
pub async fn run(_config: crate::config::AdminConfig) -> anyhow::Result<()> {
    log::info!("admin runtime is starting");
    Ok(())
}
```

Create `src/worker/mod.rs`:

```rust
pub async fn run(_config: crate::config::WorkerConfig) -> anyhow::Result<()> {
    log::info!("worker runtime is starting");
    Ok(())
}
```

For `src/coordinator/mod.rs`, `src/domain/mod.rs`, `src/executors/mod.rs`, `src/routing/mod.rs`, `src/scheduling/mod.rs`, and `src/store/mod.rs`, create this module marker:

```rust
pub const MODULE_READY: bool = true;
```

- [ ] **Step 5: Replace `src/main.rs` with subcommand dispatch**

Use this structure:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use job_scheduler::{admin, config, worker};

#[derive(Debug, Parser)]
#[command(name = "task-center")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Admin,
    Worker,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    match Cli::parse().command {
        Command::Admin => admin::run(config::AdminConfig::from_env()?).await,
        Command::Worker => worker::run(config::WorkerConfig::from_env()?).await,
    }
}
```

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt
cargo test config::tests -- --nocapture
cargo check
```

Expected: tests pass and `cargo check` exits 0.

Commit:

```bash
git add Cargo.toml src/lib.rs src/main.rs src/config.rs src/admin src/coordinator src/domain src/executors src/routing src/scheduling src/store src/worker
git commit -m "feat: add admin worker CLI foundation"
```

## Task 2: Domain Types, Labels, And State Machine

**Files:**
- Create: `src/domain/mod.rs`
- Create: `src/domain/types.rs`
- Create: `src/domain/state.rs`
- Create: `src/domain/labels.rs`

- [ ] **Step 1: Write state and label tests**

Create `src/domain/mod.rs`:

```rust
pub mod labels;
pub mod state;
pub mod types;
```

Create tests in `src/domain/state.rs` and `src/domain/labels.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_execution_flow() {
        assert!(ExecutionStatus::Scheduled.can_transition_to(ExecutionStatus::Queued));
        assert!(ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Leased));
        assert!(ExecutionStatus::Leased.can_transition_to(ExecutionStatus::Running));
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Succeeded));
    }

    #[test]
    fn rejects_success_to_running() {
        assert!(!ExecutionStatus::Succeeded.can_transition_to(ExecutionStatus::Running));
    }
}
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn selector_matches_all_required_labels() {
        let selector = LabelSelector::parse("executor=shell,trusted=true").unwrap();
        let labels = BTreeMap::from([
            ("executor".to_string(), "shell".to_string()),
            ("trusted".to_string(), "true".to_string()),
            ("region".to_string(), "cn".to_string()),
        ]);
        assert!(selector.matches(&labels));
    }

    #[test]
    fn selector_rejects_missing_label() {
        let selector = LabelSelector::parse("executor=shell,trusted=true").unwrap();
        let labels = BTreeMap::from([("executor".to_string(), "shell".to_string())]);
        assert!(!selector.matches(&labels));
    }
}
```

Run:

```bash
cargo test domain -- --nocapture
```

Expected: fail because domain types are not defined.

- [ ] **Step 2: Implement execution status transitions**

Create `src/domain/state.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Scheduled,
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    RetryWait,
    TimedOut,
    CancelRequested,
    Canceled,
    Skipped,
}

impl ExecutionStatus {
    pub fn can_transition_to(self, next: ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        matches!(
            (self, next),
            (Scheduled, Queued)
                | (Scheduled, Skipped)
                | (Queued, Leased)
                | (Queued, Skipped)
                | (Leased, Running)
                | (Leased, Queued)
                | (Running, Succeeded)
                | (Running, Failed)
                | (Running, TimedOut)
                | (Running, CancelRequested)
                | (CancelRequested, Canceled)
                | (CancelRequested, TimedOut)
                | (Failed, RetryWait)
                | (TimedOut, RetryWait)
                | (RetryWait, Queued)
                | (RetryWait, Failed)
        )
    }
}
```

- [ ] **Step 3: Implement label selector**

Create `src/domain/labels.rs`:

```rust
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelSelector {
    required: BTreeMap<String, String>,
}

impl LabelSelector {
    pub fn parse(input: &str) -> Result<Self> {
        let mut required = BTreeMap::new();
        for part in input.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid label selector `{part}`, expected key=value"))?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                return Err(anyhow!("invalid label selector `{part}`, key and value must be non-empty"));
            }
            required.insert(key.to_string(), value.to_string());
        }
        Ok(Self { required })
    }

    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        self.required
            .iter()
            .all(|(key, value)| labels.get(key) == Some(value))
    }

    pub fn required(&self) -> &BTreeMap<String, String> {
        &self.required
    }
}
```

- [ ] **Step 4: Implement serializable domain types**

Create `src/domain/types.rs` with IDs, policy enums, and DTOs:

```rust
use crate::domain::state::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Http,
    Shell,
    Builtin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    Ignore,
    FireOnceNow,
    CatchUpAll,
    CatchUpWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    Random,
    RoundRobin,
    LeastActive,
    Failover,
    Busyover,
    ConsistentHash,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    AllowParallel,
    SkipIfRunning,
    QueueIfRunning,
    FailIfRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub paused: bool,
    pub task_type: TaskType,
    pub config_json: Value,
    pub cron_expr: String,
    pub timezone: String,
    pub next_fire_at: DateTime<Utc>,
    pub misfire_policy: MisfirePolicy,
    pub misfire_grace_seconds: i64,
    pub max_retries: i32,
    pub retry_delay_seconds: i64,
    pub timeout_seconds: i64,
    pub label_selector: String,
    pub route_strategy: RouteStrategy,
    pub concurrency_policy: ConcurrencyPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Execution {
    pub id: Uuid,
    pub job_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub manual_trigger_id: Option<Uuid>,
    pub status: ExecutionStatus,
    pub idempotency_key: String,
    pub attempt_count: i32,
    pub shard_index: i32,
    pub shard_total: i32,
    pub selected_worker_id: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionAttempt {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub worker_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub status: ExecutionStatus,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerSnapshot {
    pub worker_id: String,
    pub name: Option<String>,
    pub labels: Value,
    pub capacity: i32,
    pub last_seen_snapshot: DateTime<Utc>,
    pub status_snapshot: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHeartbeat {
    pub worker_id: String,
    pub labels: BTreeMap<String, String>,
    pub capacity: usize,
    pub active_count: usize,
}
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt
cargo test domain -- --nocapture
cargo check
```

Expected: all domain tests pass and project compiles.

Commit:

```bash
git add src/domain
git commit -m "feat: add scheduler domain model"
```

## Task 3: PostgreSQL Schema And Repository Contracts

**Files:**
- Create: `db/migrations/000001_ha_scheduler.up.sql`
- Create: `db/migrations/000001_ha_scheduler.down.sql`
- Create: `src/store/mod.rs`
- Create: `src/store/postgres.rs`
- Create: `tests/common/mod.rs`
- Create: `tests/postgres_store_test.rs`
- Modify: `docker-compose.yml`

- [ ] **Step 1: Add PostgreSQL and Redis services for local integration**

Modify `docker-compose.yml` to include service names `postgres` and `redis` while preserving the current service if it exists:

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: task_center
      POSTGRES_USER: task_center
      POSTGRES_PASSWORD: task_center
    ports:
      - "5432:5432"

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
```

Run:

```bash
docker compose up -d postgres redis
```

Expected: PostgreSQL and Redis containers start.

- [ ] **Step 2: Write schema migration**

Create `db/migrations/000001_ha_scheduler.up.sql` with durable tables and indexes:

```sql
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
    status text NOT NULL,
    idempotency_key text NOT NULL,
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
    status text NOT NULL,
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
```

Create `db/migrations/000001_ha_scheduler.down.sql`:

```sql
DROP TABLE IF EXISTS admin_actions;
DROP TABLE IF EXISTS workers;
DROP TABLE IF EXISTS execution_attempts;
DROP TABLE IF EXISTS executions;
DROP TABLE IF EXISTS jobs;
```

- [ ] **Step 3: Define repository traits**

Create `src/store/mod.rs`:

```rust
use crate::domain::state::ExecutionStatus;
use crate::domain::types::{Execution, ExecutionAttempt, Job, WorkerSnapshot};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

pub mod postgres;

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn list_enabled_due(&self, now: DateTime<Utc>) -> Result<Vec<Job>>;
    async fn get(&self, id: Uuid) -> Result<Option<Job>>;
    async fn create(&self, job: CreateJob) -> Result<Job>;
    async fn update_next_fire_at(&self, id: Uuid, next_fire_at: DateTime<Utc>) -> Result<()>;
    async fn set_paused(&self, id: Uuid, paused: bool) -> Result<()>;
}

#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn create_execution(&self, input: CreateExecution) -> Result<Execution>;
    async fn get_execution(&self, id: Uuid) -> Result<Option<Execution>>;
    async fn update_status(&self, id: Uuid, status: ExecutionStatus) -> Result<()>;
    async fn create_attempt(&self, input: CreateAttempt) -> Result<ExecutionAttempt>;
    async fn finish_attempt(&self, input: FinishAttempt) -> Result<()>;
    async fn list_retry_due(&self, now: DateTime<Utc>) -> Result<Vec<Execution>>;
}

#[async_trait]
pub trait WorkerRepository: Send + Sync {
    async fn upsert_snapshot(&self, snapshot: UpsertWorkerSnapshot) -> Result<()>;
    async fn list_snapshots(&self) -> Result<Vec<WorkerSnapshot>>;
}

#[derive(Debug, Clone)]
pub struct CreateJob {
    pub name: String,
    pub task_type: crate::domain::types::TaskType,
    pub config_json: Value,
    pub cron_expr: String,
    pub next_fire_at: DateTime<Utc>,
    pub label_selector: String,
}

#[derive(Debug, Clone)]
pub struct CreateExecution {
    pub job_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub manual_trigger_id: Option<Uuid>,
    pub idempotency_key: String,
    pub shard_index: i32,
    pub shard_total: i32,
}

#[derive(Debug, Clone)]
pub struct CreateAttempt {
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub worker_id: String,
}

#[derive(Debug, Clone)]
pub struct FinishAttempt {
    pub execution_id: Uuid,
    pub attempt_no: i32,
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpsertWorkerSnapshot {
    pub worker_id: String,
    pub labels: Value,
    pub capacity: i32,
    pub status_snapshot: Value,
}
```

- [ ] **Step 4: Write repository integration tests**

Create `tests/common/mod.rs`:

```rust
use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn pg_pool() -> Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://task_center:task_center@localhost:5432/task_center".to_string());
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await?;
    sqlx::migrate!("./db/migrations").run(&pool).await?;
    Ok(pool)
}
```

Create `tests/postgres_store_test.rs`:

```rust
mod common;

use chrono::Utc;
use job_scheduler::domain::types::TaskType;
use job_scheduler::store::{CreateJob, JobRepository};
use job_scheduler::store::postgres::PostgresStore;
use serde_json::json;

#[tokio::test]
async fn creates_and_reads_job() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let job = store.create(CreateJob {
        name: format!("http_job_{}", uuid::Uuid::new_v4()),
        task_type: TaskType::Http,
        config_json: json!({"url": "https://example.com"}),
        cron_expr: "0 0 8 * * *".to_string(),
        next_fire_at: Utc::now(),
        label_selector: "executor=http".to_string(),
    }).await.unwrap();

    let loaded = store.get(job.id).await.unwrap().unwrap();
    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.label_selector, "executor=http");
}
```

Run:

```bash
docker compose up -d postgres
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center cargo test --test postgres_store_test -- --nocapture
```

Expected: fail because `PostgresStore` is not implemented.

- [ ] **Step 5: Implement `PostgresStore`**

Create `src/store/postgres.rs` with SQLx pool wrapper and implement methods used by the test first:

```rust
use super::*;
use crate::domain::types::{ConcurrencyPolicy, MisfirePolicy, RouteStrategy};
use anyhow::Result;
use sqlx::PgPool;

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobRepository for PostgresStore {
    async fn list_enabled_due(&self, now: DateTime<Utc>) -> Result<Vec<Job>> {
        let jobs = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE enabled = true AND paused = false AND next_fire_at <= $1 ORDER BY next_fire_at ASC")
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
```

Then implement the remaining repository trait methods in the same file using the SQL columns from the migration.

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center cargo test --test postgres_store_test -- --nocapture
cargo test domain config -- --nocapture
cargo check
```

Expected: PostgreSQL store test and unit tests pass.

Commit:

```bash
git add Cargo.toml docker-compose.yml db/migrations src/store tests/common tests/postgres_store_test.rs
git commit -m "feat: add postgres scheduler store"
```

## Task 4: Redis Coordinator And Dispatch Queue

**Files:**
- Create: `src/coordinator/mod.rs`
- Create: `src/coordinator/redis.rs`
- Create: `tests/redis_coordinator_test.rs`

- [ ] **Step 1: Define coordinator traits**

Create `src/coordinator/mod.rs`:

```rust
use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

pub mod redis;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub execution_id: Uuid,
    pub job_id: Uuid,
    pub label_selector: String,
    pub selected_worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub execution_id: Uuid,
    pub worker_id: String,
    pub attempt_no: i32,
}

#[async_trait]
pub trait Coordinator: Send + Sync {
    async fn try_lock(&self, key: &str, ttl: Duration) -> Result<bool>;
    async fn heartbeat(&self, heartbeat: WorkerHeartbeat, ttl: Duration) -> Result<()>;
    async fn is_worker_live(&self, worker_id: &str) -> Result<bool>;
    async fn request_cancel(&self, execution_id: Uuid, ttl: Duration) -> Result<()>;
    async fn is_cancel_requested(&self, execution_id: Uuid) -> Result<bool>;
}

#[async_trait]
pub trait DispatchQueue: Send + Sync {
    async fn enqueue(&self, item: QueueItem) -> Result<()>;
    async fn claim_for_worker(&self, worker_id: &str, labels: &std::collections::BTreeMap<String, String>, lease_ttl: Duration) -> Result<Option<Lease>>;
    async fn queue_depth(&self, queue: &str) -> Result<usize>;
}
```

- [ ] **Step 2: Write Redis tests**

Create `tests/redis_coordinator_test.rs`:

```rust
use job_scheduler::coordinator::{Coordinator, DispatchQueue, QueueItem};
use job_scheduler::coordinator::redis::RedisCoordinator;
use job_scheduler::domain::types::WorkerHeartbeat;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

async fn redis() -> RedisCoordinator {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    RedisCoordinator::connect(&url).await.unwrap()
}

#[tokio::test]
async fn lock_only_succeeds_once() {
    let redis = redis().await;
    let key = format!("test:lock:{}", Uuid::new_v4());
    assert!(redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
    assert!(!redis.try_lock(&key, Duration::from_secs(30)).await.unwrap());
}

#[tokio::test]
async fn heartbeat_marks_worker_live() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    redis.heartbeat(WorkerHeartbeat {
        worker_id: worker_id.clone(),
        labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
        capacity: 4,
        active_count: 0,
    }, Duration::from_secs(30)).await.unwrap();
    assert!(redis.is_worker_live(&worker_id).await.unwrap());
}

#[tokio::test]
async fn claim_returns_enqueued_item_once() {
    let redis = redis().await;
    let worker_id = format!("worker-{}", Uuid::new_v4());
    let labels = BTreeMap::from([("executor".to_string(), "http".to_string())]);
    let execution_id = Uuid::new_v4();
    redis.enqueue(QueueItem {
        execution_id,
        job_id: Uuid::new_v4(),
        label_selector: "executor=http".to_string(),
        selected_worker_id: Some(worker_id.clone()),
    }).await.unwrap();

    let lease = redis.claim_for_worker(&worker_id, &labels, Duration::from_secs(60)).await.unwrap().unwrap();
    assert_eq!(lease.execution_id, execution_id);
    assert!(redis.claim_for_worker(&worker_id, &labels, Duration::from_secs(60)).await.unwrap().is_none());
}
```

Run:

```bash
docker compose up -d redis
REDIS_URL=redis://127.0.0.1:6379 cargo test --test redis_coordinator_test -- --nocapture
```

Expected: fail because `RedisCoordinator` is not implemented.

- [ ] **Step 3: Implement Redis coordinator**

Create `src/coordinator/redis.rs`:

```rust
use super::{Coordinator, DispatchQueue, Lease, QueueItem};
use crate::domain::labels::LabelSelector;
use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisCoordinator {
    manager: ConnectionManager,
}

impl RedisCoordinator {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let manager = client.get_connection_manager().await?;
        Ok(Self { manager })
    }
}

#[async_trait]
impl Coordinator for RedisCoordinator {
    async fn try_lock(&self, key: &str, ttl: Duration) -> Result<bool> {
        let mut conn = self.manager.clone();
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut conn)
            .await?;
        Ok(result.is_some())
    }

    async fn heartbeat(&self, heartbeat: WorkerHeartbeat, ttl: Duration) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = format!("worker:{}", heartbeat.worker_id);
        let value = serde_json::to_string(&heartbeat)?;
        let _: () = conn.set_ex(key, value, ttl.as_secs().max(1)).await?;
        Ok(())
    }

    async fn is_worker_live(&self, worker_id: &str) -> Result<bool> {
        let mut conn = self.manager.clone();
        let key = format!("worker:{worker_id}");
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }

    async fn request_cancel(&self, execution_id: Uuid, ttl: Duration) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = format!("cancel:{execution_id}");
        let _: () = conn.set_ex(key, "1", ttl.as_secs().max(1)).await?;
        Ok(())
    }

    async fn is_cancel_requested(&self, execution_id: Uuid) -> Result<bool> {
        let mut conn = self.manager.clone();
        let key = format!("cancel:{execution_id}");
        Ok(conn.exists(key).await?)
    }
}

#[async_trait]
impl DispatchQueue for RedisCoordinator {
    async fn enqueue(&self, item: QueueItem) -> Result<()> {
        let mut conn = self.manager.clone();
        let queue = item
            .selected_worker_id
            .as_ref()
            .map(|worker_id| format!("queue:worker:{worker_id}"))
            .unwrap_or_else(|| "queue:shared".to_string());
        let value = serde_json::to_string(&item)?;
        let _: () = conn.rpush(queue, value).await?;
        Ok(())
    }

    async fn claim_for_worker(&self, worker_id: &str, labels: &BTreeMap<String, String>, lease_ttl: Duration) -> Result<Option<Lease>> {
        let queues = [format!("queue:worker:{worker_id}"), "queue:shared".to_string()];
        let mut conn = self.manager.clone();
        for queue in queues {
            let value: Option<String> = conn.lpop(&queue, None).await?;
            let Some(value) = value else { continue };
            let item: QueueItem = serde_json::from_str(&value)?;
            if !LabelSelector::parse(&item.label_selector)?.matches(labels) {
                let _: () = conn.rpush("queue:shared", value).await?;
                continue;
            }
            let lease = Lease { execution_id: item.execution_id, worker_id: worker_id.to_string(), attempt_no: 1 };
            let lease_key = format!("lease:{}", item.execution_id);
            let _: () = conn.set_ex(lease_key, serde_json::to_string(&lease)?, lease_ttl.as_secs().max(1)).await?;
            return Ok(Some(lease));
        }
        Ok(None)
    }

    async fn queue_depth(&self, queue: &str) -> Result<usize> {
        let mut conn = self.manager.clone();
        let len: usize = conn.llen(queue).await?;
        Ok(len)
    }
}
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt
REDIS_URL=redis://127.0.0.1:6379 cargo test --test redis_coordinator_test -- --nocapture
cargo test domain config -- --nocapture
cargo check
```

Expected: Redis tests pass and project compiles.

Commit:

```bash
git add src/coordinator tests/redis_coordinator_test.rs
git commit -m "feat: add redis scheduler coordinator"
```

## Task 5: Route Strategies And Scheduler Engine

**Files:**
- Create: `src/routing/mod.rs`
- Create: `src/scheduling/mod.rs`

- [ ] **Step 1: Write route tests**

Create tests in `src/routing/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn workers() -> Vec<RouteWorker> {
        vec![
            RouteWorker { worker_id: "a".to_string(), active_count: 2, capacity: 4 },
            RouteWorker { worker_id: "b".to_string(), active_count: 0, capacity: 4 },
        ]
    }

    #[test]
    fn least_active_selects_lowest_active_worker() {
        let selected = select_worker(RouteStrategyKind::LeastActive, "job-1", &workers()).unwrap();
        assert_eq!(selected.worker_id, "b");
    }

    #[test]
    fn busyover_skips_full_workers() {
        let selected = select_worker(RouteStrategyKind::Busyover, "job-1", &[
            RouteWorker { worker_id: "a".to_string(), active_count: 4, capacity: 4 },
            RouteWorker { worker_id: "b".to_string(), active_count: 1, capacity: 4 },
        ]).unwrap();
        assert_eq!(selected.worker_id, "b");
    }
}
```

Run:

```bash
cargo test routing -- --nocapture
```

Expected: fail because route types are not implemented.

- [ ] **Step 2: Implement route strategy selection**

Create `src/routing/mod.rs`:

```rust
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteStrategyKind {
    Random,
    RoundRobin,
    LeastActive,
    Failover,
    Busyover,
    ConsistentHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteWorker {
    pub worker_id: String,
    pub active_count: usize,
    pub capacity: usize,
}

pub fn select_worker(strategy: RouteStrategyKind, key: &str, workers: &[RouteWorker]) -> Result<RouteWorker> {
    if workers.is_empty() {
        return Err(anyhow!("no live workers match label selector"));
    }
    let selected = match strategy {
        RouteStrategyKind::Random => workers[0].clone(),
        RouteStrategyKind::RoundRobin => workers[0].clone(),
        RouteStrategyKind::LeastActive => workers.iter().min_by_key(|worker| worker.active_count).unwrap().clone(),
        RouteStrategyKind::Failover => workers[0].clone(),
        RouteStrategyKind::Busyover => workers
            .iter()
            .find(|worker| worker.active_count < worker.capacity)
            .ok_or_else(|| anyhow!("all matching workers are busy"))?
            .clone(),
        RouteStrategyKind::ConsistentHash => {
            let index = stable_index(key, workers.len());
            workers[index].clone()
        }
    };
    Ok(selected)
}

fn stable_index(key: &str, len: usize) -> usize {
    let hash = key.bytes().fold(0usize, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as usize));
    hash % len
}
```

- [ ] **Step 3: Write scheduler misfire tests**

Add tests to `src/scheduling/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

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
}
```

Run:

```bash
cargo test scheduling -- --nocapture
```

Expected: fail because scheduling types are not implemented.

- [ ] **Step 4: Implement misfire helper and scheduler service skeleton**

Create `src/scheduling/mod.rs`:

```rust
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisfireMode {
    Ignore,
    FireOnceNow,
    CatchUpAll,
    CatchUpWindow,
}

pub fn apply_misfire(
    mode: MisfireMode,
    mut fire_times: Vec<DateTime<Utc>>,
    now: DateTime<Utc>,
    grace_seconds: i64,
) -> Vec<DateTime<Utc>> {
    fire_times.sort();
    match mode {
        MisfireMode::Ignore => Vec::new(),
        MisfireMode::FireOnceNow => fire_times.into_iter().last().into_iter().collect(),
        MisfireMode::CatchUpAll => fire_times,
        MisfireMode::CatchUpWindow => {
            let cutoff = now - Duration::seconds(grace_seconds);
            fire_times.into_iter().filter(|time| *time >= cutoff).collect()
        }
    }
}
```

Add a `SchedulerService` struct in the same module after the helper:

```rust
pub struct SchedulerService<J, E, C, Q> {
    pub jobs: J,
    pub executions: E,
    pub coordinator: C,
    pub queue: Q,
}
```

Keep the first service commit focused on helpers and type wiring. Execution creation is added after repositories and coordinator are stable together.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt
cargo test routing scheduling -- --nocapture
cargo check
```

Expected: route and scheduling tests pass.

Commit:

```bash
git add src/routing src/scheduling
git commit -m "feat: add routing and misfire logic"
```

## Task 6: Task Executors

**Files:**
- Create: `src/executors/mod.rs`
- Create: `src/executors/http.rs`
- Create: `src/executors/shell.rs`
- Create: `src/executors/builtin.rs`
- Create: `src/executors/bugutv.rs`
- Modify: `src/jobs/bugutv.rs`
- Modify: `src/jobs/bugutv_headless.rs`

- [ ] **Step 1: Define executor trait and tests**

Create `src/executors/mod.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub mod builtin;
pub mod bugutv;
pub mod http;
pub mod shell;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub execution_id: Uuid,
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    pub shard_index: i32,
    pub shard_total: i32,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub exit_code: Option<i32>,
    pub stdout_summary: Option<String>,
    pub stderr_summary: Option<String>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, config: Value, context: ExecutionContext) -> Result<TaskOutput>;
}
```

Add shell policy tests in `src/executors/shell.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_allows_exact_command() {
        let policy = ShellPolicy::new(vec!["echo".to_string()]);
        assert!(policy.validate("echo").is_ok());
    }

    #[test]
    fn policy_rejects_unknown_command() {
        let policy = ShellPolicy::new(vec!["echo".to_string()]);
        assert!(policy.validate("rm").is_err());
    }

    #[test]
    fn truncates_output_by_bytes() {
        assert_eq!(truncate_summary("abcdef", 3), "abc");
    }
}
```

Run:

```bash
cargo test executors::shell -- --nocapture
```

Expected: fail because shell policy is not implemented.

- [ ] **Step 2: Implement shell policy and executor**

Create `src/executors/shell.rs`:

```rust
use super::{ExecutionContext, TaskExecutor, TaskOutput};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ShellPolicy {
    allowed_commands: BTreeSet<String>,
}

impl ShellPolicy {
    pub fn new(allowed_commands: Vec<String>) -> Self {
        Self { allowed_commands: allowed_commands.into_iter().collect() }
    }

    pub fn validate(&self, command: &str) -> Result<()> {
        if self.allowed_commands.contains(command) {
            return Ok(());
        }
        Err(anyhow!("shell command `{command}` is not allowed"))
    }
}

#[derive(Debug, Deserialize)]
struct ShellConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    timeout_seconds: Option<u64>,
    stdout_limit_bytes: Option<usize>,
    stderr_limit_bytes: Option<usize>,
}

pub struct ShellExecutor {
    policy: ShellPolicy,
}

impl ShellExecutor {
    pub fn new(policy: ShellPolicy) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl TaskExecutor for ShellExecutor {
    async fn execute(&self, config: Value, _context: ExecutionContext) -> Result<TaskOutput> {
        let config: ShellConfig = serde_json::from_value(config)?;
        self.policy.validate(&config.command)?;
        let mut command = Command::new(&config.command);
        command.args(config.args);
        if let Some(working_dir) = config.working_dir {
            command.current_dir(working_dir);
        }
        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(300));
        let output = tokio::time::timeout(timeout, command.output()).await??;
        Ok(TaskOutput {
            exit_code: output.status.code(),
            stdout_summary: Some(truncate_summary(&String::from_utf8_lossy(&output.stdout), config.stdout_limit_bytes.unwrap_or(4096))),
            stderr_summary: Some(truncate_summary(&String::from_utf8_lossy(&output.stderr), config.stderr_limit_bytes.unwrap_or(4096))),
            error_message: (!output.status.success()).then(|| "shell command exited with non-zero status".to_string()),
        })
    }
}

pub fn truncate_summary(input: &str, limit: usize) -> String {
    input.chars().take(limit).collect()
}
```

- [ ] **Step 3: Implement HTTP executor**

Create `src/executors/http.rs`:

```rust
use super::{ExecutionContext, TaskExecutor, TaskOutput};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct HttpConfig {
    method: String,
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<Value>,
    expected_statuses: Option<Vec<u16>>,
    timeout_seconds: Option<u64>,
}

pub struct HttpExecutor {
    client: reqwest::Client,
}

impl HttpExecutor {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

#[async_trait]
impl TaskExecutor for HttpExecutor {
    async fn execute(&self, config: Value, _context: ExecutionContext) -> Result<TaskOutput> {
        let config: HttpConfig = serde_json::from_value(config)?;
        let method: Method = config.method.parse()?;
        let mut request = self.client.request(method, config.url).timeout(Duration::from_secs(config.timeout_seconds.unwrap_or(300)));
        for (key, value) in config.headers {
            request = request.header(key, value);
        }
        if let Some(body) = config.body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let expected = config.expected_statuses.unwrap_or_else(|| (200..300).collect());
        let ok = expected.contains(&status);
        Ok(TaskOutput {
            exit_code: Some(if ok { 0 } else { 1 }),
            stdout_summary: Some(body.chars().take(4096).collect()),
            stderr_summary: None,
            error_message: (!ok).then(|| anyhow!("unexpected HTTP status {status}").to_string()),
        })
    }
}
```

- [ ] **Step 4: Implement builtin registry**

Create `src/executors/builtin.rs`:

```rust
use super::{ExecutionContext, TaskOutput};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[async_trait]
pub trait BuiltinHandler: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, config: Value, context: ExecutionContext) -> Result<TaskOutput>;
}

#[derive(Default)]
pub struct BuiltinRegistry {
    handlers: BTreeMap<String, Arc<dyn BuiltinHandler>>,
}

impl BuiltinRegistry {
    pub fn register<H: BuiltinHandler + 'static>(&mut self, handler: H) {
        self.handlers.insert(handler.name().to_string(), Arc::new(handler));
    }

    pub async fn run(&self, name: &str, config: Value, context: ExecutionContext) -> Result<TaskOutput> {
        let handler = self.handlers.get(name).ok_or_else(|| anyhow!("unknown builtin task `{name}`"))?;
        handler.run(config, context).await
    }
}
```

- [ ] **Step 5: Migrate Bugutv to config-driven builtin wrappers**

Create `src/executors/bugutv.rs` with wrapper handlers that parse JSON config and call existing job logic after adding constructors to the existing job files:

```rust
use super::builtin::BuiltinHandler;
use super::{ExecutionContext, TaskOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct BugutvConfig {
    username: String,
    password: String,
}

pub struct BugutvBuiltin;

#[async_trait]
impl BuiltinHandler for BugutvBuiltin {
    fn name(&self) -> &'static str {
        "bugutv_checkin"
    }

    async fn run(&self, config: Value, _context: ExecutionContext) -> Result<TaskOutput> {
        let config: BugutvConfig = serde_json::from_value(config)?;
        let job = crate::jobs::bugutv::BugutvCheckinJob::new_for_builtin(config.username, config.password);
        job.run_checkin_for_builtin().await?;
        Ok(TaskOutput {
            exit_code: Some(0),
            stdout_summary: Some("bugutv checkin completed".to_string()),
            stderr_summary: None,
            error_message: None,
        })
    }
}
```

Modify `src/jobs/bugutv.rs` to expose a constructor and internal run method:

```rust
impl BugutvCheckinJob {
    pub fn new_for_builtin(username: String, password: String) -> Self {
        Self { username, password, cron_expr: String::new() }
    }

    pub async fn run_checkin_for_builtin(&self) -> Result<()> {
        self.run_checkin().await
    }
}
```

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo fmt
cargo test executors -- --nocapture
cargo check
```

Expected: executor tests pass and project compiles.

Commit:

```bash
git add src/executors src/jobs/bugutv.rs src/jobs/bugutv_headless.rs
git commit -m "feat: add task executors"
```

## Task 7: Worker Runtime

**Files:**
- Create: `src/worker/mod.rs`
- Create: `tests/worker_flow_test.rs`

- [ ] **Step 1: Write worker loop test with fake queue**

Create `tests/worker_flow_test.rs`:

```rust
use job_scheduler::domain::types::WorkerHeartbeat;
use std::collections::BTreeMap;

#[test]
fn worker_heartbeat_contains_labels_and_capacity() {
    let heartbeat = WorkerHeartbeat {
        worker_id: "worker-a".to_string(),
        labels: BTreeMap::from([("executor".to_string(), "http".to_string())]),
        capacity: 4,
        active_count: 1,
    };
    assert_eq!(heartbeat.worker_id, "worker-a");
    assert_eq!(heartbeat.labels.get("executor").map(String::as_str), Some("http"));
    assert_eq!(heartbeat.capacity, 4);
    assert_eq!(heartbeat.active_count, 1);
}
```

Run:

```bash
cargo test --test worker_flow_test -- --nocapture
```

Expected: pass after Task 2. This pins the heartbeat contract before adding runtime behavior.

- [ ] **Step 2: Implement worker runtime skeleton**

Create `src/worker/mod.rs`:

```rust
use crate::config::WorkerConfig;
use crate::coordinator::redis::RedisCoordinator;
use crate::coordinator::{Coordinator, DispatchQueue};
use crate::domain::types::WorkerHeartbeat;
use anyhow::Result;
use tokio::time::MissedTickBehavior;

pub async fn run(config: WorkerConfig) -> Result<()> {
    let coordinator = RedisCoordinator::connect(&config.app.redis_url).await?;
    let mut interval = tokio::time::interval(config.heartbeat_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let heartbeat = WorkerHeartbeat {
            worker_id: config.worker_id.clone(),
            labels: config.labels.clone(),
            capacity: config.max_concurrency,
            active_count: 0,
        };
        coordinator.heartbeat(heartbeat, config.offline_after).await?;
        if let Some(lease) = coordinator
            .claim_for_worker(&config.worker_id, &config.labels, config.offline_after)
            .await?
        {
            log::info!("claimed execution {}", lease.execution_id);
        }
    }
}
```

- [ ] **Step 3: Add graceful shutdown**

Add signal handling to `src/worker/mod.rs`:

```rust
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
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
```

Use `tokio::select!` in the worker loop so the process exits on signal.

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt
cargo test --test worker_flow_test -- --nocapture
cargo check
```

Expected: worker heartbeat contract test passes and project compiles.

Commit:

```bash
git add src/worker tests/worker_flow_test.rs
git commit -m "feat: add worker runtime loop"
```

## Task 8: Admin API Foundation And Health

**Files:**
- Create: `src/admin/mod.rs`
- Create: `src/admin/auth.rs`
- Create: `src/admin/routes.rs`
- Create: `src/admin/handlers/mod.rs`
- Create: `src/admin/handlers/settings.rs`
- Create: `tests/admin_api_test.rs`

- [ ] **Step 1: Write auth and health API tests**

Create `tests/admin_api_test.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_requires_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_returns_ok_with_access_token() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

Run:

```bash
cargo test --test admin_api_test -- --nocapture
```

Expected: fail because admin routes are not implemented.

- [ ] **Step 2: Implement access token auth**

Create `src/admin/auth.rs`:

```rust
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

#[derive(Clone)]
pub struct AuthState {
    pub access_token: String,
}

pub async fn require_token(
    State(state): State<AuthState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = format!("Bearer {}", state.access_token);
    let actual = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if actual == Some(expected.as_str()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
```

- [ ] **Step 3: Implement routes and admin run**

Create `src/admin/routes.rs`:

```rust
use axum::{Router, Json, routing::get, middleware};
use serde_json::json;
use crate::admin::auth::{AuthState, require_token};

pub fn test_router(access_token: String) -> Router {
    let auth_state = AuthState { access_token };
    Router::new()
        .route("/api/health", get(health))
        .route_layer(middleware::from_fn_with_state(auth_state.clone(), require_token))
        .with_state(auth_state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}
```

Create `src/admin/mod.rs`:

```rust
pub mod auth;
pub mod handlers;
pub mod routes;

use crate::config::AdminConfig;
use anyhow::Result;

pub async fn run(config: AdminConfig) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let app = routes::test_router(config.app.access_token);
    log::info!("admin listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}
```

Create `src/admin/handlers/mod.rs`:

```rust
pub mod settings;
```

Create `src/admin/handlers/settings.rs`:

```rust
pub const SETTINGS_HANDLER_MODULE: &str = "settings";
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt
cargo test --test admin_api_test -- --nocapture
cargo check
```

Expected: admin API tests pass and project compiles.

Commit:

```bash
git add src/admin tests/admin_api_test.rs
git commit -m "feat: add admin API foundation"
```

## Task 9: Jobs, Executions, Workers, And Queues API

**Files:**
- Create: `src/admin/handlers/jobs.rs`
- Create: `src/admin/handlers/executions.rs`
- Create: `src/admin/handlers/workers.rs`
- Create: `src/admin/handlers/queues.rs`
- Modify: `src/admin/handlers/mod.rs`
- Modify: `src/admin/routes.rs`

- [ ] **Step 1: Add handler modules**

Modify `src/admin/handlers/mod.rs`:

```rust
pub mod executions;
pub mod jobs;
pub mod queues;
pub mod settings;
pub mod workers;
```

- [ ] **Step 2: Implement request/response DTOs for jobs**

Create `src/admin/handlers/jobs.rs`:

```rust
use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub name: String,
    pub task_type: String,
    pub config_json: Value,
    pub cron_expr: String,
    pub label_selector: String,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub enabled: bool,
    pub paused: bool,
}

pub async fn list_jobs() -> Json<Vec<JobResponse>> {
    Json(Vec::new())
}

pub async fn create_job(Json(_request): Json<CreateJobRequest>) -> Result<Json<JobResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

Use `501 NOT_IMPLEMENTED` only in this task to keep the API skeleton compiling. The next task wires repositories.

- [ ] **Step 3: Add execution, worker, and queue handlers**

Create each file with list endpoints returning empty JSON arrays:

```rust
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: String,
    pub status: String,
}

pub async fn list_executions() -> Json<Vec<ExecutionResponse>> {
    Json(Vec::new())
}
```

```rust
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub worker_id: String,
    pub online: bool,
}

pub async fn list_workers() -> Json<Vec<WorkerResponse>> {
    Json(Vec::new())
}
```

```rust
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct QueueResponse {
    pub name: String,
    pub depth: usize,
}

pub async fn list_queues() -> Json<Vec<QueueResponse>> {
    Json(Vec::new())
}
```

- [ ] **Step 4: Register API routes**

Modify `src/admin/routes.rs` to include:

```rust
.route("/api/jobs", get(crate::admin::handlers::jobs::list_jobs).post(crate::admin::handlers::jobs::create_job))
.route("/api/executions", get(crate::admin::handlers::executions::list_executions))
.route("/api/workers", get(crate::admin::handlers::workers::list_workers))
.route("/api/queues", get(crate::admin::handlers::queues::list_queues))
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt
cargo test --test admin_api_test -- --nocapture
cargo check
```

Expected: admin health tests pass and all API skeleton routes compile.

Commit:

```bash
git add src/admin
git commit -m "feat: add admin API resources"
```

## Task 10: Scheduling To Queue Integration

**Files:**
- Modify: `src/scheduling/mod.rs`
- Modify: `src/store/postgres.rs`
- Modify: `src/coordinator/redis.rs`
- Create: `tests/scheduler_integration_test.rs`

- [ ] **Step 1: Write duplicate prevention integration test**

Create `tests/scheduler_integration_test.rs`:

```rust
mod common;

use chrono::{Duration, Utc};
use job_scheduler::domain::types::TaskType;
use job_scheduler::scheduling::SchedulerTick;
use job_scheduler::store::{CreateJob, JobRepository};
use job_scheduler::store::postgres::PostgresStore;
use job_scheduler::coordinator::redis::RedisCoordinator;
use serde_json::json;

#[tokio::test]
async fn two_scheduler_ticks_do_not_duplicate_execution() {
    let pool = common::pg_pool().await.unwrap();
    let store = PostgresStore::new(pool);
    let redis = RedisCoordinator::connect("redis://127.0.0.1:6379").await.unwrap();
    let job = store.create(CreateJob {
        name: format!("due_job_{}", uuid::Uuid::new_v4()),
        task_type: TaskType::Http,
        config_json: json!({"url": "https://example.com"}),
        cron_expr: "0 0 8 * * *".to_string(),
        next_fire_at: Utc::now() - Duration::minutes(1),
        label_selector: "executor=http".to_string(),
    }).await.unwrap();

    let tick = SchedulerTick::new(store.clone(), redis.clone(), redis.clone());
    tick.run_once(Utc::now()).await.unwrap();
    tick.run_once(Utc::now()).await.unwrap();

    let executions = store.list_executions_for_job(job.id).await.unwrap();
    assert_eq!(executions.len(), 1);
}
```

Run:

```bash
docker compose up -d postgres redis
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center REDIS_URL=redis://127.0.0.1:6379 cargo test --test scheduler_integration_test -- --nocapture
```

Expected: fail because `SchedulerTick` and `list_executions_for_job` are not implemented.

- [ ] **Step 2: Add execution list helper**

Add to `ExecutionRepository` or an inherent test helper on `PostgresStore`:

```rust
impl PostgresStore {
    pub async fn list_executions_for_job(&self, job_id: uuid::Uuid) -> anyhow::Result<Vec<crate::domain::types::Execution>> {
        let rows = sqlx::query_as::<_, crate::domain::types::Execution>(
            "SELECT * FROM executions WHERE job_id = $1 ORDER BY created_at ASC"
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
```

Make the `pool` field visible inside the module only through this method.

- [ ] **Step 3: Implement scheduler tick**

Add to `src/scheduling/mod.rs`:

```rust
use crate::coordinator::{Coordinator, DispatchQueue, QueueItem};
use crate::store::{CreateExecution, ExecutionRepository, JobRepository};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::time::Duration as StdDuration;

#[derive(Clone)]
pub struct SchedulerTick<S, C, Q> {
    store: S,
    coordinator: C,
    queue: Q,
}

impl<S, C, Q> SchedulerTick<S, C, Q> {
    pub fn new(store: S, coordinator: C, queue: Q) -> Self {
        Self { store, coordinator, queue }
    }
}

impl<S, C, Q> SchedulerTick<S, C, Q>
where
    S: JobRepository + ExecutionRepository + Clone,
    C: Coordinator + Clone,
    Q: DispatchQueue + Clone,
{
    pub async fn run_once(&self, now: DateTime<Utc>) -> Result<()> {
        for job in self.store.list_enabled_due(now).await? {
            let lock_key = format!("lock:schedule:{}", job.id);
            if !self.coordinator.try_lock(&lock_key, StdDuration::from_secs(30)).await? {
                continue;
            }
            let execution = self.store.create_execution(CreateExecution {
                job_id: job.id,
                scheduled_at: job.next_fire_at,
                manual_trigger_id: None,
                idempotency_key: format!("{}:{}:0", job.id, job.next_fire_at.timestamp()),
                shard_index: 0,
                shard_total: 1,
            }).await?;
            self.queue.enqueue(QueueItem {
                execution_id: execution.id,
                job_id: job.id,
                label_selector: job.label_selector.clone(),
                selected_worker_id: None,
            }).await?;
            self.store.update_next_fire_at(job.id, now + chrono::Duration::minutes(1)).await?;
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center REDIS_URL=redis://127.0.0.1:6379 cargo test --test scheduler_integration_test -- --nocapture
cargo check
```

Expected: duplicate prevention test passes.

Commit:

```bash
git add src/scheduling src/store/postgres.rs tests/scheduler_integration_test.rs
git commit -m "feat: enqueue scheduled executions"
```

## Task 11: Worker Execution Finish Path

**Files:**
- Modify: `src/worker/mod.rs`
- Modify: `src/store/postgres.rs`
- Modify: `src/executors/mod.rs`

- [ ] **Step 1: Add execution result test**

Extend `tests/worker_flow_test.rs` with:

```rust
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
```

Run:

```bash
cargo test --test worker_flow_test -- --nocapture
```

Expected: pass after Task 6.

- [ ] **Step 2: Add worker execution service**

In `src/worker/mod.rs`, add a service function that executes one lease:

```rust
pub async fn execute_claimed_once<E, X>(
    executions: &E,
    executor: &X,
    execution_id: uuid::Uuid,
    attempt_no: i32,
    worker_id: String,
    config: serde_json::Value,
    context: crate::executors::ExecutionContext,
) -> anyhow::Result<()>
where
    E: crate::store::ExecutionRepository,
    X: crate::executors::TaskExecutor,
{
    executions.create_attempt(crate::store::CreateAttempt {
        execution_id,
        attempt_no,
        worker_id,
    }).await?;
    let output = executor.execute(config, context).await?;
    executions.finish_attempt(crate::store::FinishAttempt {
        execution_id,
        attempt_no,
        status: crate::domain::state::ExecutionStatus::Succeeded,
        exit_code: output.exit_code,
        stdout_summary: output.stdout_summary,
        stderr_summary: output.stderr_summary,
        error_message: output.error_message,
        duration_ms: None,
    }).await?;
    Ok(())
}
```

- [ ] **Step 3: Verify and commit**

Run:

```bash
cargo fmt
cargo test --test worker_flow_test -- --nocapture
cargo check
```

Expected: worker flow tests pass and project compiles.

Commit:

```bash
git add src/worker src/store/postgres.rs src/executors tests/worker_flow_test.rs
git commit -m "feat: record worker execution results"
```

## Task 12: Admin SPA Scaffold

**Files:**
- Create: `admin-ui/package.json`
- Create: `admin-ui/index.html`
- Create: `admin-ui/src/main.tsx`
- Create: `admin-ui/src/api.ts`
- Create: `admin-ui/src/App.tsx`
- Create: `admin-ui/src/pages/Overview.tsx`
- Create: `admin-ui/src/pages/Jobs.tsx`
- Create: `admin-ui/src/pages/Executions.tsx`
- Create: `admin-ui/src/pages/Workers.tsx`
- Create: `admin-ui/src/pages/Queues.tsx`
- Create: `admin-ui/src/pages/Settings.tsx`
- Create: `admin-ui/src/styles.css`

- [ ] **Step 1: Create SPA package**

Create `admin-ui/package.json`:

```json
{
  "scripts": {
    "dev": "vite --host 0.0.0.0",
    "build": "tsc && vite build",
    "preview": "vite preview --host 0.0.0.0"
  },
  "dependencies": {
    "@vitejs/plugin-react": "^4.3.4",
    "vite": "^6.0.0",
    "typescript": "^5.7.2",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "lucide-react": "^0.468.0"
  },
  "devDependencies": {}
}
```

Create `admin-ui/index.html`:

```html
<div id="root"></div>
<script type="module" src="/src/main.tsx"></script>
```

- [ ] **Step 2: Add API client**

Create `admin-ui/src/api.ts`:

```ts
export async function apiGet<T>(path: string): Promise<T> {
  const token = localStorage.getItem("task-center-token") ?? "";
  const response = await fetch(path, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}
```

- [ ] **Step 3: Add app shell and pages**

Create `admin-ui/src/App.tsx`:

```tsx
import { Activity, Briefcase, Clock, HardDrive, ListTree, Settings } from "lucide-react";
import "./styles.css";
import { Overview } from "./pages/Overview";
import { Jobs } from "./pages/Jobs";
import { Executions } from "./pages/Executions";
import { Workers } from "./pages/Workers";
import { Queues } from "./pages/Queues";
import { SettingsPage } from "./pages/Settings";

const pages = [
  ["Overview", Activity],
  ["Jobs", Briefcase],
  ["Executions", Clock],
  ["Workers", HardDrive],
  ["Queues", ListTree],
  ["Settings", Settings],
] as const;

export function App() {
  const current = new URLSearchParams(location.search).get("page") ?? "Overview";
  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Task Center</h1>
        {pages.map(([name, Icon]) => (
          <a className={current === name ? "active nav-item" : "nav-item"} href={`?page=${name}`} key={name}>
            <Icon size={16} />
            <span>{name}</span>
          </a>
        ))}
      </aside>
      <main className="main">
        {current === "Overview" && <Overview />}
        {current === "Jobs" && <Jobs />}
        {current === "Executions" && <Executions />}
        {current === "Workers" && <Workers />}
        {current === "Queues" && <Queues />}
        {current === "Settings" && <SettingsPage />}
      </main>
    </div>
  );
}
```

Create each page with dense operational tables. Example `admin-ui/src/pages/Overview.tsx`:

```tsx
export function Overview() {
  return (
    <>
      <h2>Overview</h2>
      <section className="metrics">
        <div>Scheduler health<br /><strong>Unknown</strong></div>
        <div>Live workers<br /><strong>0</strong></div>
        <div>Running<br /><strong>0</strong></div>
        <div>Failed<br /><strong>0</strong></div>
      </section>
      <section className="panel">Recent failures and retry queue will load from the admin API.</section>
    </>
  );
}
```

Create the remaining page components with the same exported function names used in `App.tsx`.

- [ ] **Step 4: Add styles**

Create `admin-ui/src/styles.css`:

```css
body {
  margin: 0;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #f7f8fa;
  color: #1f2937;
}

.app {
  display: grid;
  grid-template-columns: 220px 1fr;
  min-height: 100vh;
}

.sidebar {
  background: #111827;
  color: #f9fafb;
  padding: 20px 14px;
}

.sidebar h1 {
  font-size: 18px;
  margin: 0 0 18px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #d1d5db;
  text-decoration: none;
  padding: 9px 10px;
  border-radius: 6px;
}

.nav-item.active {
  background: #374151;
  color: #ffffff;
}

.main {
  padding: 24px;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(4, minmax(140px, 1fr));
  gap: 12px;
}

.metrics div,
.panel {
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 14px;
}
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cd admin-ui
npm install
npm run build
```

Expected: Vite build exits 0 and writes `admin-ui/dist`.

Commit:

```bash
git add admin-ui
git commit -m "feat: add admin SPA scaffold"
```

## Task 13: Serve Embedded SPA

**Files:**
- Create: `src/admin/static.rs`
- Modify: `src/admin/mod.rs`
- Modify: `src/admin/routes.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add static route test**

Extend `tests/admin_api_test.rs`:

```rust
#[tokio::test]
async fn unknown_ui_path_serves_index() {
    let app = job_scheduler::admin::routes::test_router("secret".to_string());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/jobs")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
}
```

Run:

```bash
cargo test --test admin_api_test -- --nocapture
```

Expected: fail because static fallback route is not registered.

- [ ] **Step 2: Serve static directory in development**

Create `src/admin/static.rs`:

```rust
use axum::Router;
use tower_http::services::ServeDir;

pub fn static_routes() -> Router {
    Router::new().fallback_service(ServeDir::new("admin-ui/dist").append_index_html_on_directories(true))
}
```

Modify `src/admin/routes.rs` to merge the static route after API routes:

```rust
.merge(crate::admin::static::static_routes())
```

Modify `src/admin/mod.rs`:

```rust
pub mod static;
```

- [ ] **Step 3: Verify and commit**

Run:

```bash
cargo fmt
cargo test --test admin_api_test -- --nocapture
cargo check
```

Expected: admin API tests pass after `admin-ui/dist` exists from Task 12.

Commit:

```bash
git add src/admin tests/admin_api_test.rs
git commit -m "feat: serve admin SPA"
```

## Task 14: Notifications

**Files:**
- Modify: `src/notify.rs`

- [ ] **Step 1: Add notifier trait**

Modify `src/notify.rs` so the existing Telegram sender implements:

```rust
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    async fn notify_job_failed(&self, job_name: &str, error: &str);
    async fn notify_retry_exhausted(&self, job_name: &str, execution_id: &str);
    async fn notify_worker_lost(&self, worker_id: &str);
}
```

- [ ] **Step 2: Keep existing start/success/failure methods as compatibility helpers**

Keep `notify_start`, `notify_success`, and `notify_failure` on `TelegramNotifier`. Implement the trait methods by calling `send` with concise messages.

- [ ] **Step 3: Verify and commit**

Run:

```bash
cargo fmt
cargo test notify -- --nocapture
cargo check
```

Expected: project compiles.

Commit:

```bash
git add src/notify.rs
git commit -m "feat: add scheduler notifier trait"
```

## Task 15: Docker, Environment, And README

**Files:**
- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `.env.example`
- Modify: `README.md`

- [ ] **Step 1: Update `.env.example`**

Add:

```dotenv
DATABASE_URL=postgres://task_center:task_center@postgres:5432/task_center
REDIS_URL=redis://redis:6379
ACCESS_TOKEN=change-me
ADMIN_BIND_ADDR=0.0.0.0:8080
WORKER_ID=worker-1
WORKER_LABELS=executor=http,executor=shell,builtin=bugutv_checkin,trusted=true
WORKER_MAX_CONCURRENCY=4
WORKER_HEARTBEAT_INTERVAL_SECONDS=10
WORKER_OFFLINE_AFTER_SECONDS=30
ENABLE_SHELL_EXECUTOR=false
SHELL_ALLOWED_COMMANDS=echo,curl
```

Remove instructions that describe env vars as the primary Bugutv job configuration path. Document that Bugutv credentials move to admin job `config_json`.

- [ ] **Step 2: Update Dockerfile**

Use a Node build stage before the Rust runtime stage:

```dockerfile
FROM node:22-alpine AS admin-ui
WORKDIR /ui
COPY admin-ui/package.json admin-ui/package-lock.json* ./
RUN npm install
COPY admin-ui ./
RUN npm run build
```

Copy `admin-ui/dist` into the Rust build context or runtime image before running admin.

- [ ] **Step 3: Update compose services**

Make `docker-compose.yml` include:

```yaml
  admin:
    build: .
    command: ["./job_scheduler", "admin"]
    env_file: .env
    depends_on:
      - postgres
      - redis
    ports:
      - "8080:8080"

  worker:
    build: .
    command: ["./job_scheduler", "worker"]
    env_file: .env
    depends_on:
      - admin
      - postgres
      - redis
```

- [ ] **Step 4: Update README**

Add sections:

```markdown
## High Availability Mode

Run `task-center admin` for scheduling, API, and the embedded admin UI. Run one or more `task-center worker` instances for execution. PostgreSQL stores durable configuration and history. Redis stores locks, heartbeats, queues, leases, and cancel flags.

## Task Types

- HTTP/Webhook tasks use `task_type=http`.
- Shell tasks use `task_type=shell` and require `ENABLE_SHELL_EXECUTOR=true` on trusted workers.
- Builtin tasks use `task_type=builtin`, for example `bugutv_checkin`.

## Execution Semantics

The scheduler provides at-least-once execution. Jobs that call external systems should use `idempotency_key` or tolerate retries.
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt
cargo check
```

If Node is available:

```bash
cd admin-ui
npm run build
```

Expected: Rust compiles and SPA builds if Node dependencies are installed.

Commit:

```bash
git add Dockerfile docker-compose.yml .env.example README.md
git commit -m "docs: add HA scheduler deployment"
```

## Task 16: Full Smoke Verification

**Files:**
- Modify: tests from prior tasks only if failures reveal a contract mismatch.

- [ ] **Step 1: Start dependencies**

Run:

```bash
docker compose up -d postgres redis
```

Expected: both services are healthy enough to accept connections.

- [ ] **Step 2: Run Rust tests**

Run:

```bash
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center REDIS_URL=redis://127.0.0.1:6379 cargo test -- --nocapture
```

Expected: all Rust unit and integration tests pass.

- [ ] **Step 3: Run Rust build**

Run:

```bash
cargo build
```

Expected: build exits 0.

- [ ] **Step 4: Run SPA build**

Run:

```bash
cd admin-ui
npm run build
```

Expected: Vite build exits 0.

- [ ] **Step 5: Run compose build**

Run:

```bash
docker compose build
```

Expected: Docker image builds with Rust binary and admin UI assets.

- [ ] **Step 6: Commit final test adjustments**

If Step 2 through Step 5 required test or documentation changes, commit those exact files:

```bash
git add tests README.md docker-compose.yml Dockerfile admin-ui src
git commit -m "test: verify HA scheduler smoke flow"
```

If there are no file changes, do not create an empty commit.

## Self-Review Checklist

- Spec coverage:
  - Admin/worker subcommands: Task 1.
  - PostgreSQL source of record: Task 3.
  - Redis coordinator: Task 4.
  - Route strategies and misfire helpers: Task 5.
  - HTTP/Shell/Builtin executors: Task 6.
  - Worker heartbeat and claim loop: Task 7.
  - Admin API: Tasks 8 and 9.
  - Scheduler-to-queue integration: Task 10.
  - Execution result reporting: Task 11.
  - Embedded SPA: Tasks 12 and 13.
  - Notifications: Task 14.
  - Deployment/docs: Task 15.
  - Smoke verification: Task 16.
- Type consistency:
  - `ExecutionStatus`, `TaskType`, `MisfirePolicy`, `RouteStrategy`, and `ConcurrencyPolicy` are defined before repository and API tasks use them.
  - `WorkerHeartbeat` is defined before Redis and worker tasks use it.
  - `TaskExecutor`, `ExecutionContext`, and `TaskOutput` are defined before worker execution uses them.
- Implementation boundaries:
  - No role-based auth, encrypted secrets, PG-only fallback, or full log storage in this first version.
  - Shell executor remains disabled unless explicitly enabled by worker configuration.
  - Existing user worktree changes must not be reverted.
