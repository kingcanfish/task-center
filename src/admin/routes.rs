use axum::{Json, Router, middleware, routing::get};
use serde_json::json;

use crate::admin::auth::{AuthState, require_token};

pub fn router(access_token: String) -> Router {
    let auth_state = AuthState { access_token };
    let api_router =
        Router::new()
            .route("/health", get(health))
            .route_layer(middleware::from_fn_with_state(
                auth_state.clone(),
                require_token,
            ));

    Router::new()
        .nest("/api", api_router)
        .with_state(auth_state)
}

pub fn test_router(access_token: String) -> Router {
    router(access_token)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
