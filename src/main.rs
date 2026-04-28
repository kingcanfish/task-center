use anyhow::Result;
use clap::{Parser, Subcommand};
use job_scheduler::{admin, config, worker};

#[derive(Debug, Parser)]
#[command(name = "task-center")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Admin,
    Worker,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    match Cli::parse().command {
        Command::Admin => admin::run(config::AdminConfig::from_env()?).await,
        Command::Worker => worker::run(config::WorkerConfig::from_env()?).await,
    }
}
