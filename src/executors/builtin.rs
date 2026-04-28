use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{ExecutionContext, TaskOutput};

#[async_trait]
pub trait BuiltinHandler: Send + Sync {
    fn name(&self) -> &'static str;

    async fn run(&self, config: Value, context: ExecutionContext) -> Result<TaskOutput>;
}

#[derive(Default)]
pub struct BuiltinRegistry {
    handlers: BTreeMap<String, Arc<dyn BuiltinHandler>>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<H: BuiltinHandler + 'static>(&mut self, handler: H) {
        self.handlers
            .insert(handler.name().to_string(), Arc::new(handler));
    }

    pub async fn run(
        &self,
        name: &str,
        config: Value,
        context: ExecutionContext,
    ) -> Result<TaskOutput> {
        let handler = self
            .handlers
            .get(name)
            .ok_or_else(|| anyhow!("unknown builtin task `{name}`"))?;

        handler.run(config, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    struct EchoHandler;

    #[async_trait]
    impl BuiltinHandler for EchoHandler {
        fn name(&self) -> &'static str {
            "echo"
        }

        async fn run(&self, config: Value, _context: ExecutionContext) -> Result<TaskOutput> {
            Ok(TaskOutput {
                exit_code: Some(0),
                stdout_summary: Some(config.to_string()),
                stderr_summary: None,
                error_message: None,
            })
        }
    }

    fn context() -> ExecutionContext {
        ExecutionContext {
            execution_id: Uuid::nil(),
            scheduled_at: Utc::now(),
            shard_index: 0,
            shard_total: 1,
            cancel: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn registry_runs_registered_handler() {
        let mut registry = BuiltinRegistry::new();
        registry.register(EchoHandler);

        let output = registry
            .run("echo", serde_json::json!({"ok": true}), context())
            .await
            .unwrap();

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout_summary, Some(r#"{"ok":true}"#.to_string()));
    }

    #[tokio::test]
    async fn registry_rejects_unknown_handler() {
        let registry = BuiltinRegistry::new();

        let error = registry
            .run("missing", serde_json::json!({}), context())
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "unknown builtin task `missing`");
    }
}
