use anyhow::Result;
use reqwest::Client;
use std::env;

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
