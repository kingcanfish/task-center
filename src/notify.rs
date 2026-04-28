use anyhow::Result;
use reqwest::Client;
use std::env;

#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    async fn notify_job_failed(&self, job_name: &str, error: &str);
    async fn notify_retry_exhausted(&self, job_name: &str, execution_id: &str);
    async fn notify_worker_lost(&self, worker_id: &str);
}

/// Telegram 通知器
#[derive(Clone)]
pub struct TelegramNotifier {
    client: Client,
    bot_token: String,
    chat_id: String,
}

impl TelegramNotifier {
    /// 从环境变量创建通知器
    pub fn from_env() -> Option<Self> {
        let bot_token = env::var("TELEGRAM_BOT_TOKEN").ok()?;
        let chat_id = env::var("TELEGRAM_CHAT_ID").ok()?;

        log::info!("Telegram 通知已启用");
        Some(Self {
            client: Client::new(),
            bot_token,
            chat_id,
        })
    }

    /// 发送消息
    pub async fn send(&self, message: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        let params = [
            ("chat_id", self.chat_id.as_str()),
            ("text", message),
            ("parse_mode", "HTML"),
        ];

        let resp = self.client.post(&url).form(&params).send().await?;

        if resp.status().is_success() {
            log::debug!("Telegram 消息发送成功");
        } else {
            let body = resp.text().await?;
            log::error!("Telegram 消息发送失败: {}", body);
        }

        Ok(())
    }

    /// 发送任务开始通知
    pub async fn notify_start(&self, job_name: &str) {
        let message = format!("🚀 <b>任务开始</b>\n任务: {}", job_name);
        if let Err(e) = self.send(&message).await {
            log::error!("发送开始通知失败: {}", e);
        }
    }

    /// 发送任务成功通知
    pub async fn notify_success(&self, job_name: &str, details: Option<&str>) {
        let mut message = format!("✅ <b>任务成功</b>\n任务: {}", job_name);
        if let Some(details) = details {
            message.push_str(&format!("\n\n{}", details));
        }
        if let Err(e) = self.send(&message).await {
            log::error!("发送成功通知失败: {}", e);
        }
    }

    /// 发送任务失败通知
    pub async fn notify_failure(&self, job_name: &str, error: &str) {
        let message = format!("❌ <b>任务失败</b>\n任务: {}\n错误: {}", job_name, error);
        if let Err(e) = self.send(&message).await {
            log::error!("发送失败通知失败: {}", e);
        }
    }
}

#[async_trait::async_trait]
impl Notifier for TelegramNotifier {
    async fn notify_job_failed(&self, job_name: &str, error: &str) {
        let message = format!("❌ <b>任务失败</b>\n任务: {}\n错误: {}", job_name, error);
        if let Err(e) = self.send(&message).await {
            log::error!("发送任务失败通知失败: {}", e);
        }
    }

    async fn notify_retry_exhausted(&self, job_name: &str, execution_id: &str) {
        let message = format!(
            "⚠️ <b>任务重试已耗尽</b>\n任务: {}\n执行: {}",
            job_name, execution_id
        );
        if let Err(e) = self.send(&message).await {
            log::error!("发送重试耗尽通知失败: {}", e);
        }
    }

    async fn notify_worker_lost(&self, worker_id: &str) {
        let message = format!("⚠️ <b>Worker 离线</b>\nWorker: {}", worker_id);
        if let Err(e) = self.send(&message).await {
            log::error!("发送 Worker 离线通知失败: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Notifier, TelegramNotifier};

    #[test]
    fn telegram_notifier_implements_scheduler_notifier() {
        fn assert_notifier<T: Notifier>() {}

        assert_notifier::<TelegramNotifier>();
    }
}
