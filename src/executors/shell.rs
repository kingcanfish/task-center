use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;

use super::{ExecutionContext, TaskExecutor, TaskOutput};

const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 4096;

#[derive(Debug, Clone)]
pub struct ShellPolicy {
    allowed_commands: BTreeSet<String>,
}

impl ShellPolicy {
    pub fn new(allowed_commands: Vec<String>) -> Self {
        Self {
            allowed_commands: allowed_commands.into_iter().collect(),
        }
    }

    pub fn validate(&self, command: &str) -> Result<()> {
        if self.allowed_commands.contains(command) {
            Ok(())
        } else {
            Err(anyhow!("shell command `{command}` is not allowed"))
        }
    }
}

pub struct ShellExecutor {
    policy: ShellPolicy,
}

impl ShellExecutor {
    pub fn new(policy: ShellPolicy) -> Self {
        Self { policy }
    }
}

#[derive(Debug, Deserialize)]
struct ShellConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    timeout_seconds: Option<u64>,
    stdout_limit_bytes: Option<usize>,
    stderr_limit_bytes: Option<usize>,
}

#[async_trait]
impl TaskExecutor for ShellExecutor {
    async fn execute(
        &self,
        config: serde_json::Value,
        context: ExecutionContext,
    ) -> Result<TaskOutput> {
        let config: ShellConfig = serde_json::from_value(config)?;
        self.policy.validate(&config.command)?;

        let mut command = Command::new(&config.command);
        command.args(&config.args);
        if let Some(working_dir) = config.working_dir {
            command.current_dir(working_dir);
        }
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_process_group(&mut command);

        let stdout_limit = config
            .stdout_limit_bytes
            .unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES);
        let stderr_limit = config
            .stderr_limit_bytes
            .unwrap_or(DEFAULT_OUTPUT_LIMIT_BYTES);
        let timeout =
            Duration::from_secs(config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS));

        let mut child = command.spawn()?;
        let process_group_id = child.id();
        let stdout_reader = spawn_summary_reader(child.stdout.take(), stdout_limit);
        let stderr_reader = spawn_summary_reader(child.stderr.take(), stderr_limit);

        let outcome = tokio::select! {
            status = child.wait() => ShellOutcome::Exited(status?.code()),
            _ = tokio::time::sleep(timeout) => {
                kill_child_tree(&mut child, process_group_id).await?;
                ShellOutcome::TimedOut
            }
            _ = context.cancel.cancelled() => {
                kill_child_tree(&mut child, process_group_id).await?;
                ShellOutcome::Cancelled
            }
        };
        terminate_process_group(process_group_id);

        let stdout_summary = join_summary(stdout_reader).await?;
        let stderr_summary = join_summary(stderr_reader).await?;

        Ok(TaskOutput {
            exit_code: outcome.exit_code(),
            stdout_summary: Some(stdout_summary),
            stderr_summary: Some(stderr_summary),
            error_message: outcome.error_message(),
        })
    }
}

enum ShellOutcome {
    Exited(Option<i32>),
    TimedOut,
    Cancelled,
}

impl ShellOutcome {
    fn exit_code(&self) -> Option<i32> {
        match self {
            Self::Exited(exit_code) => *exit_code,
            Self::TimedOut | Self::Cancelled => None,
        }
    }

    fn error_message(&self) -> Option<String> {
        match self {
            Self::Exited(Some(0)) => None,
            Self::Exited(_) => Some("shell command exited with non-zero status".to_string()),
            Self::TimedOut => Some("shell command timed out".to_string()),
            Self::Cancelled => Some("shell command cancelled".to_string()),
        }
    }
}

async fn kill_child_tree(
    child: &mut tokio::process::Child,
    process_group_id: Option<u32>,
) -> Result<()> {
    terminate_process_group(process_group_id);
    if child.id().is_some() {
        child.start_kill()?;
    }
    let _ = child.wait().await;
    Ok(())
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(process_group_id: Option<u32>) {
    const SIGTERM: i32 = 15;
    const SIGKILL: i32 = 9;

    let Some(process_group_id) = process_group_id else {
        return;
    };
    let Ok(process_group_id) = i32::try_from(process_group_id) else {
        return;
    };

    // Negative pid targets a process group. The child is started in a new group.
    unsafe {
        let _ = kill(-process_group_id, SIGTERM);
        let _ = kill(-process_group_id, SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_process_group_id: Option<u32>) {}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

fn spawn_summary_reader<R>(reader: Option<R>, limit: usize) -> JoinHandle<Result<String>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(mut reader) = reader else {
            return Ok(String::new());
        };

        let mut stored = Vec::with_capacity(limit.min(DEFAULT_OUTPUT_LIMIT_BYTES));
        let mut buffer = [0_u8; 8192];
        loop {
            let read = reader.read(&mut buffer).await?;
            if read == 0 {
                break;
            }

            if stored.len() < limit {
                let remaining = limit - stored.len();
                stored.extend_from_slice(&buffer[..read.min(remaining)]);
            }
        }

        Ok(summary_from_bytes(&stored))
    })
}

async fn join_summary(mut handle: JoinHandle<Result<String>>) -> Result<String> {
    match tokio::time::timeout(Duration::from_secs(1), &mut handle).await {
        Ok(result) => result?,
        Err(_) => {
            handle.abort();
            Ok(String::new())
        }
    }
}

fn summary_from_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_string(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).to_string()
        }
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    }
}

pub fn truncate_summary(input: &str, limit: usize) -> String {
    if input.len() <= limit {
        return input.to_string();
    }

    let mut end = limit;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    fn context(cancel: CancellationToken) -> ExecutionContext {
        ExecutionContext {
            execution_id: Uuid::nil(),
            scheduled_at: Utc::now(),
            shard_index: 0,
            shard_total: 1,
            cancel,
        }
    }

    #[test]
    fn policy_allows_exact_command() {
        let policy = ShellPolicy::new(vec!["echo".to_string()]);

        assert!(policy.validate("echo").is_ok());
    }

    #[test]
    fn policy_rejects_unknown_command() {
        let policy = ShellPolicy::new(vec!["echo".to_string()]);

        assert!(policy.validate("rm").is_err());
    }

    #[test]
    fn truncates_output_by_bytes() {
        assert_eq!(truncate_summary("abcdef", 3), "abc");
    }

    #[test]
    fn truncates_output_without_splitting_utf8() {
        assert_eq!(truncate_summary("你abc", 4), "你a");
    }

    #[tokio::test]
    async fn executor_cancels_running_child() {
        let executor = ShellExecutor::new(ShellPolicy::new(vec!["sh".to_string()]));
        let cancel = CancellationToken::new();
        cancel.cancel();

        let output = executor
            .execute(
                json!({
                    "command": "sh",
                    "args": ["-c", "sleep 5"],
                }),
                context(cancel),
            )
            .await
            .unwrap();

        assert_eq!(output.exit_code, None);
        assert_eq!(
            output.error_message,
            Some("shell command cancelled".to_string())
        );
    }

    #[tokio::test]
    async fn executor_does_not_wait_for_background_descendant_pipe() {
        let executor = ShellExecutor::new(ShellPolicy::new(vec!["sh".to_string()]));
        let output = tokio::time::timeout(
            Duration::from_secs(1),
            executor.execute(
                json!({
                    "command": "sh",
                    "args": ["-c", "sleep 5 &"],
                }),
                context(CancellationToken::new()),
            ),
        )
        .await
        .expect("executor should not wait for background descendant")
        .unwrap();

        assert_eq!(output.exit_code, Some(0));
    }
}
