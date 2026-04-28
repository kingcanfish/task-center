use super::Job;
use anyhow::Result;
use async_trait::async_trait;
use headless_chrome::LaunchOptionsBuilder;
use headless_chrome::browser::Browser;
use log::info;
use std::env;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::task;
use tokio_util::sync::CancellationToken;

/// 布谷TV签到任务（无头浏览器版本）
#[derive(Clone)]
pub struct BugutvHeadlessCheckinJob {
    username: String,
    password: String,
    cron_expr: String,
}

#[async_trait]
impl Job for BugutvHeadlessCheckinJob {
    fn name(&self) -> &str {
        "bugutv_headless_checkin"
    }

    fn cron_expr(&self) -> &str {
        &self.cron_expr
    }

    fn from_env() -> Option<Self> {
        let username = env::var("BUGUTV_USERNAME").ok()?;
        let password = env::var("BUGUTV_PASSWORD").ok()?;
        let cron_expr =
            env::var("BUGUTV_HEADLESS_CRON").unwrap_or_else(|_| "0 0 8 * * *".to_string());

        info!("从环境变量加载 BugutvHeadlessCheckinJob 配置");
        Some(Self {
            username,
            password,
            cron_expr,
        })
    }

    async fn run(&self) -> Result<()> {
        info!("[{}] 开始执行无头浏览器签到任务", self.name());

        match self.run_checkin_for_builtin().await {
            Ok(_) => {
                info!("[{}] 签到任务执行成功", self.name());
                Ok(())
            }
            Err(e) => {
                info!("[{}] 签到任务执行失败: {e}", self.name());
                Err(e)
            }
        }
    }
}

impl BugutvHeadlessCheckinJob {
    pub fn new_for_builtin(username: String, password: String) -> Self {
        Self {
            username,
            password,
            cron_expr: String::new(),
        }
    }

    pub async fn run_checkin_for_builtin(&self) -> Result<()> {
        self.run_checkin_for_builtin_with_cancel(CancellationToken::new())
            .await
    }

    pub async fn run_checkin_for_builtin_with_cancel(
        &self,
        cancel: CancellationToken,
    ) -> Result<()> {
        let job = self.clone();
        task::spawn_blocking(move || job.run_checkin_blocking(cancel)).await?
    }

    fn run_checkin_blocking(&self, cancel: CancellationToken) -> Result<()> {
        check_cancelled(&cancel)?;
        let (browser, tab, _cancel_guard) = self.launch_browser_with_cancel(&cancel)?;
        check_cancelled(&cancel)?;
        info!("无头浏览器启动成功");
        self.run_checkin_with_browser(&browser, &tab, &cancel)
    }

    #[cfg(test)]
    fn launch_browser(
        &self,
    ) -> Result<(Browser, std::sync::Arc<headless_chrome::browser::tab::Tab>)> {
        let (browser, tab, _cancel_guard) =
            self.launch_browser_with_cancel(&CancellationToken::new())?;
        Ok((browser, tab))
    }

    fn launch_browser_with_cancel(
        &self,
        cancel: &CancellationToken,
    ) -> Result<(
        Browser,
        std::sync::Arc<headless_chrome::browser::tab::Tab>,
        BrowserCancelGuard,
    )> {
        check_cancelled(cancel)?;
        let launch_options = LaunchOptionsBuilder::default()
            .headless(true)
            .window_size(Some((1280, 720)))
            .build()?;

        // 启动浏览器，捕获错误并提供详细的错误信息
        let browser = Browser::new(launch_options).map_err(|e| {
            anyhow::anyhow!(
                "无法启动无头浏览器: {}\n\n可能的原因:\n\
                1. 系统未安装 Chrome/Chromium 浏览器\n\
                2. Chrome/Chromium 不在系统 PATH 中\n\
                \n\
                解决方案:\n\
                - Ubuntu/Debian: sudo apt-get install chromium-browser\n\
                - Fedora: sudo dnf install chromium\n\
                - Arch: sudo pacman -S chromium\n\
                - macOS: brew install chromium\n\
                \n\
                或者设置环境变量指定 Chrome 路径: CHROME_PATH=/path/to/chrome",
                e
            )
        })?;
        let cancel_guard = BrowserCancelGuard::new(browser.get_process_id(), cancel.clone());
        check_cancelled(cancel)?;

        // 创建新标签页
        let tab = browser
            .new_tab()
            .map_err(|e| anyhow::anyhow!("无法创建浏览器标签页: {}", e))?;
        check_cancelled(cancel)?;

        info!("无头浏览器启动成功 (窗口大小: 1280x720)");
        Ok((browser, tab, cancel_guard))
    }

    fn run_checkin_with_browser(
        &self,
        _browser: &Browser,
        tab: &std::sync::Arc<headless_chrome::browser::tab::Tab>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        check_cancelled(cancel)?;
        // 访问主页
        info!("正在访问布谷TV主页...");
        tab.navigate_to("https://www.bugutv.vip")?;
        tab.wait_until_navigated()?;

        // 等待页面加载
        cancellable_sleep(Duration::from_secs(3), cancel)?;
        check_cancelled(cancel)?;

        // 检查是否被Cloudflare拦截
        let title = tab.get_title()?;
        if title.contains("Just a moment") || title.contains("Cloudflare") {
            info!("检测到Cloudflare挑战页面，智能等待验证...");

            // 智能等待，最多重试5次，每次等待时间递增
            let mut retry_count = 0;
            let max_retries = 5;

            while retry_count < max_retries {
                let wait_time = 5 + retry_count * 3; // 5, 8, 11, 14, 17秒
                info!("等待 {} 秒后检查验证状态...", wait_time);
                cancellable_sleep(Duration::from_secs(wait_time), cancel)?;

                let current_title = tab.get_title()?;
                if !current_title.contains("Just a moment") && !current_title.contains("Cloudflare")
                {
                    info!("成功通过Cloudflare验证！");
                    break;
                }

                retry_count += 1;
                if retry_count >= max_retries {
                    return Err(anyhow::anyhow!(
                        "无法通过Cloudflare验证，已重试{}次",
                        max_retries
                    ));
                }
                info!(
                    "仍在验证中，继续等待... ({}/{})",
                    retry_count + 1,
                    max_retries
                );
            }
        }

        // 登录
        self.login_with_browser(tab, cancel)?;
        check_cancelled(cancel)?;

        // 获取签到前积分
        let point_before = self.get_point_with_browser(tab, cancel)?;
        check_cancelled(cancel)?;

        // 执行签到
        self.check_with_browser(tab, cancel)?;
        check_cancelled(cancel)?;

        // 获取签到后积分
        let point_after = self.get_point_with_browser(tab, cancel)?;

        let earned = point_after - point_before;
        info!("***************布谷TV无头浏览器签到:结果统计***************");
        info!("{} 本次获得积分: {earned} 个", self.username);
        info!("累计积分: {point_after} 个");
        info!("****************************************************************");

        Ok(())
    }

    fn login_with_browser(
        &self,
        tab: &std::sync::Arc<headless_chrome::browser::tab::Tab>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        check_cancelled(cancel)?;
        info!("正在执行登录操作...");

        // 检查是否有登录表单元素
        tab.wait_for_elements("#login-form, .login-form, [name='user_login']")?;

        // 填写用户名
        tab.wait_for_element("#user_login, [name='username'], input[name='username']")?
            .click()
            .ok();
        tab.type_str(&self.username)?;

        // 填写密码
        tab.wait_for_element("#user_pass, [name='password'], input[name='password']")?
            .click()
            .ok();
        tab.type_str(&self.password)?;

        // 点击登录按钮
        tab.wait_for_element("#wp-submit, [name='wp-submit'], .login-submit input[type='submit']")?
            .click()?;

        // 等待登录完成
        cancellable_sleep(Duration::from_secs(3), cancel)?;
        check_cancelled(cancel)?;

        // 检查是否登录成功（检查当前URL是否包含user）
        let current_url = tab.get_url();
        if current_url.contains("/user") {
            info!("登录成功，已跳转到用户页面");
            Ok(())
        } else {
            // 检查页面是否显示登录成功信息
            let page_source = tab.get_content()?;
            if page_source.contains("登录成功") || page_source.contains("success") {
                info!("登录成功");
                Ok(())
            } else {
                Err(anyhow::anyhow!("登录失败"))
            }
        }
    }

    fn get_point_with_browser(
        &self,
        tab: &std::sync::Arc<headless_chrome::browser::tab::Tab>,
        cancel: &CancellationToken,
    ) -> Result<i32> {
        check_cancelled(cancel)?;
        info!("正在获取积分信息...");

        // 确保在用户页面
        let current_url = tab.get_url();
        if !current_url.contains("/user") {
            tab.navigate_to("https://www.bugutv.vip/user")?;
            tab.wait_until_navigated()?;
        }

        // 等待页面加载
        cancellable_sleep(Duration::from_secs(2), cancel)?;
        check_cancelled(cancel)?;

        // 查找积分元素
        let elements = tab.wait_for_elements(
            ".badge-warning-lighten, [class*='badge'][class*='warning'], [class*='coins']",
        )?;

        if let Some(element) = elements.first() {
            // 获取包含积分的文本
            let text = element.get_inner_text()?;

            // 解析积分数量
            if let Some(point) = parse_point_text(&text) {
                info!("当前积分: {}", point);
                return Ok(point);
            }
        } else {
            info!("未找到积分元素，尝试从页面源码解析...");
        }

        let page_source = tab.get_content()?;
        extract_point_from_html(&page_source).ok_or_else(|| anyhow::anyhow!("未找到积分信息"))
    }

    fn check_with_browser(
        &self,
        tab: &std::sync::Arc<headless_chrome::browser::tab::Tab>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        check_cancelled(cancel)?;
        info!("正在执行签到操作...");

        // 确保在用户页面
        let current_url = tab.get_url();
        if !current_url.contains("/user") {
            tab.navigate_to("https://www.bugutv.vip/user")?;
            tab.wait_until_navigated()?;
        }

        // 等待页面加载
        cancellable_sleep(Duration::from_secs(2), cancel)?;
        check_cancelled(cancel)?;

        // 查找签到按钮或相关元素
        let signin_selectors = [
            "[data-nonce]",
            ".qiandao",
            ".checkin",
            "[id*='check']",
            "button[type='submit']",
            "input[type='submit']",
        ];

        for selector in &signin_selectors {
            match tab.wait_for_element(selector) {
                Ok(element) => {
                    info!("找到签到相关元素: {}", selector);

                    // 尝试点击按钮
                    element.click()?;

                    // 等待处理结果
                    cancellable_sleep(Duration::from_secs(3), cancel)?;
                    check_cancelled(cancel)?;

                    // 检查页面内容判断签到结果
                    let page_source = tab.get_content()?;

                    if page_source.contains("今日已签到") || page_source.contains("已签到")
                    {
                        info!("今日已签到，请明日再来");
                        return Ok(());
                    } else if page_source.contains("签到成功") || page_source.contains("签到奖励")
                    {
                        info!("签到成功，奖励已到账");
                        return Ok(());
                    } else {
                        info!("签到操作完成，但状态未知");
                        return Ok(());
                    }
                }
                Err(_) => continue,
            }
        }

        Err(anyhow::anyhow!("未找到签到按钮或元素"))
    }
}

struct BrowserCancelGuard {
    done: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

impl BrowserCancelGuard {
    fn new(process_id: Option<u32>, cancel: CancellationToken) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let watcher_done = Arc::clone(&done);
        let watcher = thread::spawn(move || {
            while !watcher_done.load(Ordering::Relaxed) {
                if cancel.is_cancelled() {
                    terminate_browser_process(process_id);
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        Self {
            done,
            watcher: Some(watcher),
        }
    }
}

impl Drop for BrowserCancelGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

fn check_cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(anyhow::anyhow!("bugutv headless checkin cancelled"))
    } else {
        Ok(())
    }
}

fn cancellable_sleep(duration: Duration, cancel: &CancellationToken) -> Result<()> {
    let step = Duration::from_millis(100);
    let mut remaining = duration;
    while !remaining.is_zero() {
        check_cancelled(cancel)?;
        let current = remaining.min(step);
        thread::sleep(current);
        remaining -= current;
    }
    check_cancelled(cancel)
}

#[cfg(unix)]
fn terminate_browser_process(process_id: Option<u32>) {
    let Some(process_id) = process_id else {
        return;
    };
    let Ok(process_id) = i32::try_from(process_id) else {
        return;
    };

    const SIGTERM: i32 = 15;
    unsafe {
        let _ = kill(process_id, SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate_browser_process(_process_id: Option<u32>) {}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

fn parse_point_text(text: &str) -> Option<i32> {
    text.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<i32>()
        .ok()
}

fn extract_point_from_html(page_source: &str) -> Option<i32> {
    let patterns = [
        r#"<span[^>]*class="[^"]*badge-warning[^"]*"[^>]*>.*?(\d+).*?</span>"#,
        r#"积分[：:]\s*(\d+)"#,
        r#"(?i)points?[：:]\s*(\d+)"#,
    ];

    for pattern in patterns {
        let Ok(regex) = regex::Regex::new(pattern) else {
            continue;
        };
        let Some(cap) = regex.captures(page_source) else {
            continue;
        };
        let Some(point_str) = cap.get(1) else {
            continue;
        };
        if let Ok(point) = point_str.as_str().parse::<i32>() {
            info!("从页面源码获取到积分: {} (pattern: {})", point, pattern);
            return Some(point);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试无头浏览器启动
    #[tokio::test]
    async fn test_headless_browser_launch() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .try_init()
            .ok();

        if env::var("RUN_BUGUTV_HEADLESS_TESTS").as_deref() != Ok("1") {
            info!("未设置 RUN_BUGUTV_HEADLESS_TESTS=1，跳过无头浏览器启动测试");
            return;
        }

        let job = BugutvHeadlessCheckinJob::from_env()
            .expect("RUN_BUGUTV_HEADLESS_TESTS=1 时必须设置 BUGUTV_USERNAME 和 BUGUTV_PASSWORD");

        let (_browser, _tab) = job.launch_browser().expect("无头浏览器启动失败");
        info!("无头浏览器功能正常");
    }

    /// 测试无头浏览器访问网站（需要设置环境变量）
    /// 运行: BUGUTV_USERNAME=xxx BUGUTV_PASSWORD=xxx cargo test test_headless_browser_visit -- --nocapture
    #[tokio::test]
    async fn test_headless_browser_visit() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .try_init()
            .ok();

        if env::var("RUN_BUGUTV_HEADLESS_TESTS").as_deref() != Ok("1") {
            info!("未设置 RUN_BUGUTV_HEADLESS_TESTS=1，跳过无头浏览器访问测试");
            return;
        }

        let job = BugutvHeadlessCheckinJob::from_env()
            .expect("RUN_BUGUTV_HEADLESS_TESTS=1 时必须设置 BUGUTV_USERNAME 和 BUGUTV_PASSWORD");

        info!("正在测试完整无头浏览器签到流程...");
        job.run_checkin_blocking(CancellationToken::new())
            .expect("无头浏览器签到流程测试失败");
        info!("测试完成");
    }

    #[tokio::test]
    async fn cancelled_builtin_returns_before_launching_browser() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let job = BugutvHeadlessCheckinJob::new_for_builtin("user".to_string(), "pass".to_string());
        let error = job
            .run_checkin_for_builtin_with_cancel(cancel)
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "bugutv headless checkin cancelled");
    }

    #[test]
    fn parses_points_from_badge_text() {
        assert_eq!(parse_point_text("积分 123 个"), Some(123));
    }

    #[test]
    fn extracts_points_from_badge_html() {
        let html =
            r#"<span class="badge badge-warning-lighten"><i class="fas fa-coins"></i> 42</span>"#;

        assert_eq!(extract_point_from_html(html), Some(42));
    }
}
