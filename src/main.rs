mod jobs;
mod notify;
mod scheduler;

use anyhow::Result;
use std::sync::Arc;

use jobs::bugutv::BugutvCheckinJob;
use jobs::Job;
use scheduler::Scheduler;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志记录器
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    log::info!("启动 Job Scheduler");

    // 创建调度器（自动初始化 Telegram 通知器）
    let scheduler = Scheduler::new().await?;

    // 注册 Bugutv 签到任务
    if let Some(job) = BugutvCheckinJob::from_env() {
        scheduler.register(Arc::new(job)).await?;
    } else {
        log::warn!("未配置 BUGUTV_USERNAME 或 BUGUTV_PASSWORD，跳过 Bugutv 签到任务");
    }

    // TODO: 在这里注册更多任务
    // if let Some(job) = AnotherJob::from_env() {
    //     scheduler.register(Arc::new(job)).await?;
    // }

    // 启动调度器并等待 Ctrl+C 信号
    scheduler.start_and_wait().await?;

    Ok(())
}
