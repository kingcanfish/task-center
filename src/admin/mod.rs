pub mod auth;
pub mod handlers;
pub mod routes;
pub mod state;
pub mod r#static;

use crate::config::AdminConfig;
use crate::coordinator::redis::RedisCoordinator;
use crate::scheduling::SchedulerTick;
use crate::store::postgres::PostgresStore;
use crate::{admin::state::AdminApiState, admin::state::AppState};
use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use tokio::time::MissedTickBehavior;

pub async fn run(config: AdminConfig) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.app.database_url)
        .await?;
    sqlx::migrate!("./db/migrations").run(&pool).await?;

    let store = PostgresStore::new(pool);
    let coordinator = RedisCoordinator::connect(&config.app.redis_url).await?;
    spawn_scheduler_loop(store.clone(), coordinator.clone());

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let app = routes::router_with_state(
        config.app.access_token,
        AppState::with_api(AdminApiState::new(store, coordinator)),
    );
    log::info!("admin listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn spawn_scheduler_loop(store: PostgresStore, coordinator: RedisCoordinator) {
    tokio::spawn(async move {
        let tick = SchedulerTick::new(store, coordinator.clone(), coordinator);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            if let Err(err) = tick.run_once(chrono::Utc::now()).await {
                log::error!("scheduler tick failed: {err:#}");
            }
        }
    });
}
