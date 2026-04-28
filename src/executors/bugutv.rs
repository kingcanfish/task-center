use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::builtin::BuiltinHandler;
use super::{ExecutionContext, TaskOutput};

#[derive(Deserialize)]
struct BugutvConfig {
    username: String,
    password: String,
}

pub struct BugutvBuiltin;

#[async_trait]
impl BuiltinHandler for BugutvBuiltin {
    fn name(&self) -> &'static str {
        "bugutv_checkin"
    }

    async fn run(
        &self,
        config: serde_json::Value,
        context: ExecutionContext,
    ) -> Result<TaskOutput> {
        let config: BugutvConfig = serde_json::from_value(config)?;
        if context.cancel.is_cancelled() {
            return Ok(cancelled_output("bugutv checkin cancelled"));
        }

        let job = crate::jobs::bugutv::BugutvCheckinJob::new_for_builtin(
            config.username,
            config.password,
        );
        tokio::select! {
            result = job.run_checkin_for_builtin() => result?,
            _ = context.cancel.cancelled() => return Ok(cancelled_output("bugutv checkin cancelled")),
        }

        Ok(TaskOutput {
            exit_code: Some(0),
            stdout_summary: Some("bugutv checkin completed".to_string()),
            stderr_summary: None,
            error_message: None,
        })
    }
}

pub struct BugutvHeadlessBuiltin;

#[async_trait]
impl BuiltinHandler for BugutvHeadlessBuiltin {
    fn name(&self) -> &'static str {
        "bugutv_headless_checkin"
    }

    async fn run(
        &self,
        config: serde_json::Value,
        context: ExecutionContext,
    ) -> Result<TaskOutput> {
        let config: BugutvConfig = serde_json::from_value(config)?;
        if context.cancel.is_cancelled() {
            return Ok(cancelled_output("bugutv headless checkin cancelled"));
        }

        let job = crate::jobs::bugutv_headless::BugutvHeadlessCheckinJob::new_for_builtin(
            config.username,
            config.password,
        );
        if let Err(error) = job
            .run_checkin_for_builtin_with_cancel(context.cancel.clone())
            .await
        {
            if is_headless_cancelled(&error) {
                return Ok(cancelled_output("bugutv headless checkin cancelled"));
            }
            return Err(error);
        }

        Ok(TaskOutput {
            exit_code: Some(0),
            stdout_summary: Some("bugutv headless checkin completed".to_string()),
            stderr_summary: None,
            error_message: None,
        })
    }
}

fn is_headless_cancelled(error: &anyhow::Error) -> bool {
    error.to_string() == "bugutv headless checkin cancelled"
}

fn cancelled_output(message: &str) -> TaskOutput {
    TaskOutput {
        exit_code: None,
        stdout_summary: None,
        stderr_summary: None,
        error_message: Some(message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
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

    #[tokio::test]
    async fn headless_builtin_returns_cancelled_without_launching_browser() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let output = BugutvHeadlessBuiltin
            .run(
                serde_json::json!({
                    "username": "user",
                    "password": "pass",
                }),
                context(cancel),
            )
            .await
            .unwrap();

        assert_eq!(output.exit_code, None);
        assert_eq!(
            output.error_message,
            Some("bugutv headless checkin cancelled".to_string())
        );
    }
}
