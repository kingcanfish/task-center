use anyhow::Result;
use chrono_tz::Tz;
use std::env;
use std::sync::Arc;
use tokio_cron_scheduler::{Job as CronJob, JobScheduler};
use tokio_util::sync::CancellationToken;

use crate::jobs::Job;
use crate::notify::TelegramNotifier;

/// 调度器 - 管理所有定时任务
pub struct Scheduler {
    inner: JobScheduler,
    timezone: Tz,
    notifier: Option<Arc<TelegramNotifier>>,
}

impl Scheduler {
    pub async fn new() -> Result<Self> {
        let inner = JobScheduler::new().await?;

        // 从环境变量读取时区，默认为 Asia/Shanghai
        let tz_str = env::var("TZ").unwrap_or_else(|_| "Asia/Shanghai".to_string());
        let timezone: Tz = tz_str.parse().unwrap_or(chrono_tz::Asia::Shanghai);
        log::info!("调度器时区: {}", timezone);

        // 初始化 Telegram 通知器
        let notifier = TelegramNotifier::from_env().map(Arc::new);

        Ok(Self {
            inner,
            timezone,
            notifier,
        })
    }

    /// 注册一个 Job
    pub async fn register<J: Job + 'static>(&self, job: Arc<J>) -> Result<()> {
        let job_name = job.name().to_string();
        let cron_expr = job.cron_expr().to_string();
        let timezone = self.timezone;
        let notifier = self.notifier.clone();

        let cron_job = CronJob::new_async_tz(cron_expr.as_str(), timezone, move |_uuid, _l| {
            let job = job.clone();
            let job_name = job.name().to_string();
            let notifier = notifier.clone();
            Box::pin(async move {
                log::info!("[{}] 定时任务触发", job_name);

                // 发送开始通知
                if let Some(ref n) = notifier {
                    n.notify_start(&job_name).await;
                }

                match job.run().await {
                    Ok(_) => {
                        log::info!("[{}] 任务执行完成", job_name);
                        // 发送成功通知
                        if let Some(ref n) = notifier {
                            n.notify_success(&job_name, None).await;
                        }
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        log::error!("[{}] 任务执行失败: {}", job_name, error_msg);
                        // 发送失败通知
                        if let Some(ref n) = notifier {
                            n.notify_failure(&job_name, &error_msg).await;
                        }
                    }
                }
            })
        })?;

        self.inner.add(cron_job).await?;
        log::info!(
            "已注册任务: {} (cron: {}, tz: {})",
            job_name,
            cron_expr,
            timezone
        );

        Ok(())
    }

    /// 启动调度器并等待退出信号
    pub async fn start_and_wait(mut self) -> Result<()> {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        self.inner.set_shutdown_handler(Box::new(move || {
            let token_for_future = token_clone.clone();
            Box::pin(async move {
                log::warn!("Scheduler shut down.");
                token_for_future.cancel();
            })
        }));

        // 监听 SIGINT 和 SIGTERM 信号
        let token_for_signals = token.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
            tokio::select! {
                _ = sigint.recv() => {
                    log::info!("收到 SIGINT 信号，正在关闭...");
                }
                _ = sigterm.recv() => {
                    log::info!("收到 SIGTERM 信号，正在关闭...");
                }
            }
            token_for_signals.cancel();
        });

        self.inner.start().await?;
        log::info!("调度器已启动，按 Ctrl+C 或发送 SIGTERM 退出...");
        token.cancelled().await;

        log::info!("调度器已关闭");
        // tokio::time::sleep(Duration::from_secs(100)).await;

        Ok(())
    }
}
