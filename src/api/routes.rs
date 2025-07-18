use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::storage::ConfigStore;
use super::handlers;

pub fn create_router(store: ConfigStore) -> Router {
    Router::new()
        // Route endpoints
        .route("/routes", post(handlers::create_route))
        .route("/routes", get(handlers::list_routes))
        .route("/routes/:id", get(handlers::get_route))
        .route("/routes/:id", delete(handlers::delete_route))
        
        // Cluster endpoints
        .route("/clusters", post(handlers::create_cluster))
        .route("/clusters", get(handlers::list_clusters))
        .route("/clusters/:name", get(handlers::get_cluster))
        .route("/clusters/:name", delete(handlers::delete_cluster))
        
        // Config generation
        .route("/generate-config", post(handlers::generate_envoy_config))
        
        // Health check
        .route("/health", get(health_check))
        
        // Share the store across all handlers
        .with_state(store)
}

async fn health_check() -> &'static str {
    "OK"
}