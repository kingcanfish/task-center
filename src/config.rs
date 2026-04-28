use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub access_token: String,
}

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub app: AppConfig,
    pub bind_addr: String,
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub app: AppConfig,
    pub worker_id: String,
    pub labels: BTreeMap<String, String>,
    pub max_concurrency: usize,
    pub heartbeat_interval: Duration,
    pub offline_after: Duration,
    pub enable_shell_executor: bool,
    pub shell_allowed_commands: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = env_required("DATABASE_URL")?;
        let redis_url = env_required("REDIS_URL")?;
        let access_token = env_required("ACCESS_TOKEN")?;
        validate_access_token(&access_token)?;
        Ok(Self {
            database_url,
            redis_url,
            access_token,
        })
    }
}

impl AdminConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            app: AppConfig::from_env()?,
            bind_addr: env::var("ADMIN_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        })
    }
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let app = AppConfig::from_env()?;
        let worker_id = env_required("WORKER_ID")?;
        let labels = parse_env_labels(&env::var("WORKER_LABELS").unwrap_or_default())?;
        let max_concurrency = env::var("WORKER_MAX_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4);
        let heartbeat_interval = env_seconds("WORKER_HEARTBEAT_INTERVAL_SECONDS", 10);
        let offline_after = env_seconds("WORKER_OFFLINE_AFTER_SECONDS", 30);
        let enable_shell_executor = env::var("ENABLE_SHELL_EXECUTOR")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let shell_allowed_commands = env::var("SHELL_ALLOWED_COMMANDS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|v| {
                let trimmed = v.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .collect();
        Ok(Self {
            app,
            worker_id,
            labels,
            max_concurrency,
            heartbeat_interval,
            offline_after,
            enable_shell_executor,
            shell_allowed_commands,
        })
    }
}

fn env_required(name: &str) -> Result<String> {
    let value = env::var(name).map_err(|_| anyhow!("{name} is required"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("{name} is required"));
    }
    Ok(value)
}

fn env_seconds(name: &str, default: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default))
}

pub fn validate_access_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        return Err(anyhow!("ACCESS_TOKEN is required"));
    }
    Ok(())
}

pub fn parse_env_labels(input: &str) -> Result<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    for item in input
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid worker label `{item}`, expected key=value"))?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(anyhow!(
                "invalid worker label `{item}`, expected non-empty key and value"
            ));
        }
        labels.insert(key.to_string(), value.to_string());
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worker_labels() {
        let labels = parse_env_labels("executor=http,region=cn,trusted=true").unwrap();
        assert_eq!(labels.get("executor").map(String::as_str), Some("http"));
        assert_eq!(labels.get("region").map(String::as_str), Some("cn"));
        assert_eq!(labels.get("trusted").map(String::as_str), Some("true"));
    }

    #[test]
    fn rejects_empty_access_token() {
        let err = validate_access_token("").unwrap_err();
        assert!(err.to_string().contains("ACCESS_TOKEN"));
    }
}
