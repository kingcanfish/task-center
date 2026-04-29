use crate::coordinator::redis::RedisCoordinator;
use crate::store::postgres::PostgresStore;

#[derive(Clone)]
pub struct AdminApiState {
    pub store: PostgresStore,
    pub coordinator: RedisCoordinator,
}

impl AdminApiState {
    pub fn new(store: PostgresStore, coordinator: RedisCoordinator) -> Self {
        Self { store, coordinator }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub api: Option<AdminApiState>,
}

impl AppState {
    pub fn empty() -> Self {
        Self { api: None }
    }

    pub fn with_api(api: AdminApiState) -> Self {
        Self { api: Some(api) }
    }
}
