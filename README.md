# Job Scheduler

一个可扩展的定时任务调度器，基于 `tokio-cron-scheduler` 实现，支持时区配置和 Telegram 通知。

## 项目结构

```
src/
├── main.rs          # 入口，注册和启动任务
├── scheduler.rs     # 调度器实现（支持时区和通知）
├── notify.rs        # Telegram 通知模块
└── jobs/
    ├── mod.rs       # Job trait 定义
    └── bugutv.rs    # 布谷TV签到任务
```

## 功能特性

- ✅ 基于 Cron 表达式的定时任务调度
- ✅ 时区支持（默认 Asia/Shanghai）
- ✅ Telegram 机器人通知（任务开始/成功/失败）
- ✅ 可扩展的 Job 架构

## 添加新任务

1. 在 `src/jobs/` 目录下创建新文件
2. 实现 `Job` trait：

```rust
use async_trait::async_trait;
use anyhow::Result;
use std::env;
use super::Job;

pub struct MyJob {
    cron_expr: String,
}

#[async_trait]
impl Job for MyJob {
    fn name(&self) -> &str { "my_job" }
    fn cron_expr(&self) -> &str { &self.cron_expr }

    fn from_env() -> Option<Self> {
        let config = env::var("MY_JOB_CONFIG").ok()?;
        Some(Self { cron_expr: "0 0 * * * *".to_string() })
    }

    async fn run(&self) -> Result<()> {
        // 任务逻辑
        Ok(())
    }
}
```

3. 在 `src/jobs/mod.rs` 中导出模块
4. 在 `src/main.rs` 中注册任务

## 环境变量

### 全局配置

| 变量名 | 说明 | 必填 | 默认值 |
|--------|------|------|--------|
| `TZ` | 时区 | 否 | `Asia/Shanghai` |

### Telegram 通知

| 变量名 | 说明 | 必填 |
|--------|------|------|
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | 否 |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | 否 |

> 如果不设置 Telegram 相关环境变量，通知功能将自动禁用。

### Bugutv 签到任务

| 变量名 | 说明 | 必填 | 默认值 |
|--------|------|------|--------|
| `BUGUTV_USERNAME` | 布谷TV用户名 | 是 | - |
| `BUGUTV_PASSWORD` | 布谷TV密码 | 是 | - |
| `BUGUTV_CRON` | Cron表达式 | 否 | `0 0 8 * * *` |

## Cron 表达式格式

```
秒 分 时 日 月 周
```

示例：
- `0 0 8 * * *` - 每天早上8点
- `0 30 7 * * *` - 每天早上7:30
- `0 0 */6 * * *` - 每6小时执行一次

## 运行

```bash
export TZ="Asia/Shanghai"
export TELEGRAM_BOT_TOKEN="your_bot_token"
export TELEGRAM_CHAT_ID="your_chat_id"
export BUGUTV_USERNAME="your_username"
export BUGUTV_PASSWORD="your_password"

cargo run --release
```

## Docker Compose 运行（推荐）

1. 复制环境变量模板：
```bash
cp .env.example .env
```

2. 编辑 `.env` 文件，填入你的配置

3. 启动服务：
```bash
docker compose up -d
```

4. 查看日志：
```bash
docker compose logs -f
```

## Docker 运行

```bash
docker build -t job-scheduler .
docker run -d \
  -e TZ="Asia/Shanghai" \
  -e TELEGRAM_BOT_TOKEN="your_bot_token" \
  -e TELEGRAM_CHAT_ID="your_chat_id" \
  -e BUGUTV_USERNAME="your_username" \
  -e BUGUTV_PASSWORD="your_password" \
  job-scheduler
```

## 通知效果

任务执行时会发送以下通知：

- 🚀 **任务开始** - 任务触发时
- ✅ **任务成功** - 任务执行成功
- ❌ **任务失败** - 任务执行失败（包含错误信息）
