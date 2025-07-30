use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tower_http::cors::{CorsLayer, Any};

use super::handlers;
use crate::storage::ConfigStore;
use crate::xds::SimpleXdsServer;

#[derive(Clone)]
pub struct AppState {
    pub store: ConfigStore,
    pub xds_server: SimpleXdsServer,
}

pub fn create_router(store: ConfigStore, xds_server: SimpleXdsServer) -> Router {
    let app_state = AppState { store, xds_server };

    Router::new()
        // Route endpoints
        .route("/routes", post(handlers::create_route))
        .route("/routes", get(handlers::list_routes))
        .route("/routes/:id", get(handlers::get_route))
        .route("/routes/:id", put(handlers::update_route))
        .route("/routes/:id", delete(handlers::delete_route))
        // Cluster endpoints
        .route("/clusters", post(handlers::create_cluster))
        .route("/clusters", get(handlers::list_clusters))
        .route("/clusters/:name", get(handlers::get_cluster))
        .route("/clusters/:name", put(handlers::update_cluster))
        .route("/clusters/:name", delete(handlers::delete_cluster))
        // Config generation
        .route("/generate-config", post(handlers::generate_envoy_config))
        .route(
            "/generate-bootstrap",
            get(handlers::generate_bootstrap_config),
        )
        // HTTP methods
        .route("/supported-http-methods", get(handlers::get_supported_http_methods))
        // Health check
        .route("/health", get(health_check))
        // Add CORS middleware to allow frontend access
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        )
        // Share the app state across all handlers
        .with_state(app_state)
}

async fn health_check() -> &'static str {
    "OK"
}
