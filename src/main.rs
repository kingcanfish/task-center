use reqwest::Client;
use reqwest::cookie::Jar;
use std::sync::Arc;
use regex::Regex;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;

const BASE_URL: &str = "https://www.bugutv.vip";

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志记录器，默认日志级别为info
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    log::info!("开始运行 bugutv 自动签到脚本");

    let username = env::var("BUGUTV_USERNAME")
        .map_err(|_| anyhow::anyhow!("请设置环境变量 BUGUTV_USERNAME"))?;
    let password = env::var("BUGUTV_PASSWORD")
        .map_err(|_| anyhow::anyhow!("请设置环境变量 BUGUTV_PASSWORD"))?;

    for i in 0..3 {
        if i > 0 {
            log::info!("尝试第 {} 次...", i + 1);
        }

        match run_checkin(&username, &password).await {
            Ok(_) => break,
            Err(e) => {
                log::error!("签到失败: {e}");
                sleep(Duration::from_secs(10)).await;
            }
        }
    }

    Ok(())
}

async fn run_checkin(username: &str, password: &str) -> Result<()> {
    let jar = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_provider(jar)
        .build()?;

    // 登录
    if !login(&client, username, password).await? {
        return Err(anyhow::anyhow!("登录失败"));
    }

    // 获取签到前积分
    let point_before = get_point(&client).await?;
    
    // 执行签到
    check(&client).await?;

    // 获取签到后积分
    let point_after = get_point(&client).await?;

    let earned = point_after - point_before;
    log::info!("***************布谷TV签到:结果统计***************");
    log::info!("{username} 本次获得积分: {earned} 个");
    log::info!("累计积分: {point_after} 个");
    log::info!("**************************************************");

    // 退出登录
    logout(&client).await?;

    Ok(())
}

async fn login(client: &Client, username: &str, password: &str) -> Result<bool> {
    log::info!("准备登录...");

    // 预请求主页（获取 cookie）
    client.get(BASE_URL).send().await?;
    sleep(Duration::from_secs(1)).await;

    // 登录请求
    let params = [
        ("action", "user_login"),
        ("username", username),
        ("password", password),
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

async fn get_point(client: &Client) -> Result<i32> {
    let resp = client
        .get(format!("{BASE_URL}/user"))
        .send()
        .await?;

    let body = resp.text().await?;
    sleep(Duration::from_secs(1)).await;

    let re = Regex::new(r#"<span class="badge badge-warning-lighten"><i class="fas fa-coins"></i> (.*?)</span>"#)?;
    let cap = re.captures(&body).ok_or(anyhow::anyhow!("未找到积分信息"))?;
    
    let point = cap[1].parse::<i32>()?;
    Ok(point)
}

async fn check(client: &Client) -> Result<()> {
    let resp = client
        .get(format!("{BASE_URL}/user"))
        .send()
        .await?;

    let body = resp.text().await?;
    sleep(Duration::from_secs(1)).await;

    let re = Regex::new(r#"data-nonce="(.*?)""#)?;
    let cap = re.captures(&body).ok_or(anyhow::anyhow!("未获取到 data-nonce"))?;
    let nonce = &cap[1];
    log::info!("准备签到，data-nonce: {nonce}");

    let params = [
        ("action", "user_qiandao"),
        ("nonce", nonce),
    ];

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

async fn logout(client: &Client) -> Result<()> {
    let resp = client
        .get(format!("{BASE_URL}/user"))
        .send()
        .await?;

    let body = resp.text().await?;
    
    let re = Regex::new(r#"action=logout&amp;redirect_to=https%3A%2F%2Fwww.bugutv.vip&amp;_wpnonce=(.*?)""#)?;
    let cap = re.captures(&body).ok_or(anyhow::anyhow!("未获取到 wpnonce，无法退出登录"))?;
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