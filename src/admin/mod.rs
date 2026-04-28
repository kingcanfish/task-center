pub mod auth;
pub mod handlers;
pub mod routes;
pub mod r#static;

use crate::config::AdminConfig;
use anyhow::Result;

pub async fn run(config: AdminConfig) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let app = routes::router(config.app.access_token);
    log::info!("admin listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}
