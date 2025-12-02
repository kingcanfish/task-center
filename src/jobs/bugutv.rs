use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::Client;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use super::Job;

const BASE_URL: &str = "https://www.bugutv.vip";

/// 布谷TV签到任务
pub struct BugutvCheckinJob {
    username: String,
    password: String,
    cron_expr: String,
}

#[async_trait]
impl Job for BugutvCheckinJob {
    fn name(&self) -> &str {
        "bugutv_checkin"
    }

    fn cron_expr(&self) -> &str {
        &self.cron_expr
    }

    fn from_env() -> Option<Self> {
        let username = env::var("BUGUTV_USERNAME").ok()?;
        let password = env::var("BUGUTV_PASSWORD").ok()?;
        let cron_expr = env::var("BUGUTV_CRON").unwrap_or_else(|_| "0 0 8 * * *".to_string());
        
        log::info!("从环境变量加载 BugutvCheckinJob 配置");
        Some(Self {
            username,
            password,
            cron_expr,
        })
    }

    async fn run(&self) -> Result<()> {
        log::info!("[{}] 开始执行签到任务", self.name());

        for i in 0..3 {
            if i > 0 {
                log::info!("[{}] 尝试第 {} 次...", self.name(), i + 1);
            }

            match self.run_checkin().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    log::error!("[{}] 签到失败: {e}", self.name());
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }

        Err(anyhow::anyhow!("签到失败，已重试3次"))
    }
}

impl BugutvCheckinJob {
    async fn run_checkin(&self) -> Result<()> {
        let jar = Arc::new(Jar::default());
        let client = Client::builder().cookie_provider(jar).build()?;

        // 登录
        if !self.login(&client).await? {
            return Err(anyhow::anyhow!("登录失败"));
        }

        // 获取签到前积分
        let point_before = self.get_point(&client).await?;

        // 执行签到
        self.check(&client).await?;

        // 获取签到后积分
        let point_after = self.get_point(&client).await?;

        let earned = point_after - point_before;
        log::info!("***************布谷TV签到:结果统计***************");
        log::info!("{} 本次获得积分: {earned} 个", self.username);
        log::info!("累计积分: {point_after} 个");
        log::info!("**************************************************");

        // 退出登录
        self.logout(&client).await?;

        Ok(())
    }

    async fn login(&self, client: &Client) -> Result<bool> {
        log::info!("准备登录...");

        // 预请求主页（获取 cookie）
        client.get(BASE_URL).send().await?;
        sleep(Duration::from_secs(1)).await;

        // 登录请求
        let params = [
            ("action", "user_login"),
            ("username", self.username.as_str()),
            ("password", self.password.as_str()),
            ("rememberme", "1"),
        ];

        let resp = client
            .post(format!("{BASE_URL}/wp-admin/admin-ajax.php"))
            .form(&params)
            .send()
            .await?;

        let body = resp.text().await?;

        if body.contains("登录成功") || body.contains("\\u767b\\u5f55\\u6210\\u529f") {
            log::info!("登录成功");
            Ok(true)
        } else {
            log::info!("登录失败");
            log::info!("body: {body}");
            Ok(false)
        }
    }

    async fn get_point(&self, client: &Client) -> Result<i32> {
        let resp = client.get(format!("{BASE_URL}/user")).send().await?;

        let body = resp.text().await?;
        sleep(Duration::from_secs(1)).await;

        let re = Regex::new(
            r#"<span class="badge badge-warning-lighten"><i class="fas fa-coins"></i> (.*?)</span>"#,
        )?;
        let cap = re
            .captures(&body)
            .ok_or(anyhow::anyhow!("未找到积分信息"))?;

        let point = cap[1].parse::<i32>()?;
        Ok(point)
    }

    async fn check(&self, client: &Client) -> Result<()> {
        let resp = client.get(format!("{BASE_URL}/user")).send().await?;

        let body = resp.text().await?;
        sleep(Duration::from_secs(1)).await;

        let re = Regex::new(r#"data-nonce="(.*?)""#)?;
        let cap = re
            .captures(&body)
            .ok_or(anyhow::anyhow!("未获取到 data-nonce"))?;
        let nonce = &cap[1];
        log::info!("准备签到，data-nonce: {nonce}");

        let params = [("action", "user_qiandao"), ("nonce", nonce)];

        let resp = client
            .post(format!("{BASE_URL}/wp-admin/admin-ajax.php"))
            .form(&params)
            .send()
            .await?;

        let body = resp.text().await?;
        sleep(Duration::from_secs(1)).await;

        let content = &body;
        if content.contains("\\u4eca\\u65e5\\u5df2\\u7b7e\\u5230") {
            log::info!("今日已签到，请明日再来");
        } else if content.contains("\\u7b7e\\u5230\\u6210\\u529f") {
            log::info!("签到成功，奖励已到账：1.0积分");
        } else {
            log::info!("签到失败，返回内容: {content}");
        }

        Ok(())
    }

    async fn logout(&self, client: &Client) -> Result<()> {
        let resp = client.get(format!("{BASE_URL}/user")).send().await?;

        let body = resp.text().await?;

        let re = Regex::new(
            r#"action=logout&redirect_to=https%3A%2F%2Fwww.bugutv.vip&_wpnonce=(.*?)""#,
        )?;
        let cap = re
            .captures(&body)
            .ok_or(anyhow::anyhow!("未获取到 wpnonce，无法退出登录"))?;
        let wpnonce = &cap[1];

        let logout_url = format!(
            "{BASE_URL}/wp-login.php?action=logout&redirect_to=https%3A%2F%2Fwww.bugutv.vip&_wpnonce={wpnonce}"
        );

        match client.get(logout_url).send().await {
            Ok(_) => log::info!("退出登录成功"),
            Err(e) => log::error!("退出登录失败: {e}"),
        }

        Ok(())
    }
}
