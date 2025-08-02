use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::cors::{CorsLayer, Any};

use super::handlers;
use crate::auth::JwtKeys;
use crate::auth_handlers;
use crate::auth_middleware::{auth_middleware, optional_auth_middleware};
use crate::rbac::RbacEnforcer;
use crate::storage::ConfigStore;
use crate::xds::SimpleXdsServer;

#[derive(Clone)]
pub struct AppState {
    pub store: ConfigStore,
    pub xds_server: SimpleXdsServer,
    pub jwt_keys: JwtKeys,
    pub rbac: RbacEnforcer,
}

pub fn create_router(
    store: ConfigStore,
    xds_server: SimpleXdsServer,
    jwt_keys: JwtKeys,
    rbac: RbacEnforcer,
) -> Router {
    let app_state = AppState {
        store,
        xds_server,
        jwt_keys: jwt_keys.clone(),
        rbac: rbac.clone(),
    };

    // Protected routes that require full authentication & authorization
    let protected_routes = Router::new()
        // Route management (write operations)
        .route("/routes", post(handlers::create_route))
        .route("/routes/{id}", put(handlers::update_route))
        .route("/routes/{id}", delete(handlers::delete_route))
        // Cluster management (write operations)
        .route("/clusters", post(handlers::create_cluster))
        .route("/clusters/{name}", put(handlers::update_cluster))
        .route("/clusters/{name}", delete(handlers::delete_cluster))
        // Config generation (sensitive operations)
        .route("/generate-config", post(handlers::generate_envoy_config))
        .route("/generate-bootstrap", get(handlers::generate_bootstrap_config))
        // Apply full authentication + authorization middleware
        .layer(middleware::from_fn_with_state(
            (jwt_keys.clone(), rbac.clone()),
            auth_middleware,
        ));

    // Authentication routes (public - no auth required)
    let auth_routes = Router::new()
        .route("/auth/login", post(auth_handlers::login))
        .route("/auth/logout", post(auth_handlers::logout))
        .route("/auth/health", get(auth_handlers::auth_health))
        .with_state(app_state.clone()); // Use the same AppState

    // Protected auth routes (require authentication)
    let protected_auth_routes = Router::new()
        .route("/auth/me", get(auth_handlers::get_user_info))
        // Apply full authentication + authorization middleware
        .layer(middleware::from_fn_with_state(
            (jwt_keys.clone(), rbac.clone()),
            auth_middleware,
        ))
        .with_state(app_state.clone()); // Use the same AppState

    // Public or read-only routes with optional authentication
    let public_routes = Router::new()
        // Read operations (can be public or authenticated for better logging)
        .route("/routes", get(handlers::list_routes))
        .route("/routes/{id}", get(handlers::get_route))
        .route("/clusters", get(handlers::list_clusters))
        .route("/clusters/{name}", get(handlers::get_cluster))
        // System info (public)
        .route("/supported-http-methods", get(handlers::get_supported_http_methods))
        .route("/health", get(health_check))
        // Apply optional authentication middleware (logs user if authenticated)
        .layer(middleware::from_fn_with_state(
            jwt_keys.clone(),
            optional_auth_middleware,
        ));

    // Combine all routes
    Router::new()
        .merge(protected_routes)
        .merge(protected_auth_routes)
        .merge(auth_routes)
        .merge(public_routes)
        // Add CORS middleware to allow frontend access
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // Share the app state across all handlers
        .with_state(app_state)
}

async fn health_check() -> &'static str {
    "OK"
}
