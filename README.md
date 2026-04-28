# Job Scheduler

一个 Rust 实现的高可用任务调度服务。服务拆成 `admin` 和 `worker` 两类进程：`admin` 负责调度、任务配置、监控 API 和内置 Web UI，`worker` 负责上报心跳、领取任务并执行。

## 项目结构

```
src/
├── admin/          # Admin API、鉴权、静态 SPA 服务
├── coordinator/    # Redis 锁、心跳、队列、租约、取消标记
├── domain/         # Job、Execution、Attempt、Worker 等领域模型
├── executors/      # HTTP、Shell、Builtin 任务执行器
├── scheduling/     # Cron 扫描、误触发策略、派发逻辑
├── store/          # PostgreSQL 持久化仓储
├── worker/         # Worker 心跳、领取任务和执行循环
├── config.rs       # 环境变量配置
└── notify.rs       # Telegram 通知适配
admin-ui/           # React/Vite 管理控制台
db/migrations/      # PostgreSQL schema
```

## High Availability Mode

Run the `admin` subcommand for scheduling, API, and the embedded admin UI. Run one or more `worker` instances for execution. PostgreSQL stores durable configuration and history. Redis stores locks, heartbeats, queues, leases, and cancel flags.

Admin 和 worker 使用同一个二进制：

```bash
cargo run --release -- admin
cargo run --release -- worker
```

也可以先构建再直接运行二进制：

```bash
cargo build --release
./target/release/job_scheduler admin
./target/release/job_scheduler worker
```

多 worker 部署时，每个实例必须使用唯一的 `WORKER_ID`。通过 `WORKER_LABELS` 控制可接收的任务，例如 `executor=http,trusted=true` 或 `executor=shell,trusted=true`。

## Task Types

- HTTP/Webhook tasks use `task_type=http`.
- Shell tasks use `task_type=shell` and require `ENABLE_SHELL_EXECUTOR=true` on trusted workers.
- Builtin tasks use `task_type=builtin`, for example `bugutv_checkin` or `bugutv_headless_checkin`.

Bugutv 凭据不再作为主要环境变量配置。创建 builtin 任务时，把凭据写入任务的 `config_json`：

```json
{
  "username": "your_username",
  "password": "your_password"
}
```

`bugutv_headless_checkin` 需要 worker 所在环境具备 Chrome/Chromium；如需手动指定浏览器路径，可设置 `CHROME_PATH`。

## Execution Semantics

The scheduler provides at-least-once execution. Jobs that call external systems should use `idempotency_key` or tolerate retries.

Worker 领取任务后通过租约执行；如果 worker 掉线或租约过期，任务可能被重新派发。Shell executor 默认关闭，只应在可信 worker 上启用。

## 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| `DATABASE_URL` | PostgreSQL 连接串 | 必填 |
| `REDIS_URL` | Redis 连接串 | 必填 |
| `ACCESS_TOKEN` | Admin API Bearer Token | 必填 |
| `ADMIN_BIND_ADDR` | Admin 监听地址 | `0.0.0.0:8080` |
| `WORKER_ID` | Worker 唯一 ID | 必填 |
| `WORKER_LABELS` | Worker 标签，逗号分隔的 `key=value` | 空 |
| `WORKER_MAX_CONCURRENCY` | Worker 最大并发 | `4` |
| `WORKER_HEARTBEAT_INTERVAL_SECONDS` | 心跳间隔 | `10` |
| `WORKER_OFFLINE_AFTER_SECONDS` | worker 离线判定 TTL | `30` |
| `ENABLE_SHELL_EXECUTOR` | 是否启用 shell executor | `false` |
| `SHELL_ALLOWED_COMMANDS` | shell 命令白名单 | 空 |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | 可选 |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | 可选 |

## Docker Compose 运行

1. 复制环境变量模板：

```bash
cp .env.example .env
```

2. 编辑 `.env`，至少修改 `ACCESS_TOKEN`，并为每个 worker 设置唯一 `WORKER_ID`。

3. 启动 PostgreSQL、Redis、admin 和 worker：

```bash
docker compose up -d
```

4. 查看日志：

```bash
docker compose logs -f admin worker
```

Admin UI 默认监听 `http://localhost:8080`。API 请求需要携带 `Authorization: Bearer <ACCESS_TOKEN>`。

## 本地开发

启动依赖：

```bash
docker compose up -d postgres redis
```

运行测试时使用本地依赖地址：

```bash
DATABASE_URL=postgres://task_center:task_center@localhost:5432/task_center \
REDIS_URL=redis://127.0.0.1:6379 \
cargo test -- --nocapture
```

构建管理端：

```bash
cd admin-ui
pnpm install
pnpm run build
```
