pub mod bugutv;

use anyhow::Result;
use async_trait::async_trait;

/// Job trait - 所有定时任务都需要实现此 trait
#[async_trait]
pub trait Job: Send + Sync {
    /// 任务名称
    fn name(&self) -> &str;
    
    /// Cron 表达式
    fn cron_expr(&self) -> &str;
    
    /// 执行任务
    async fn run(&self) -> Result<()>;
    
    /// 从环境变量创建任务，返回 None 表示配置不完整
    fn from_env() -> Option<Self> where Self: Sized;
}
