use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::cors::CorsLayer;
use axum::http::{Method, HeaderName};

use super::handlers;
use crate::auth::JwtKeys;
use crate::auth_handlers;
use crate::auth_middleware::{auth_middleware, optional_auth_middleware};
use crate::config::AppConfig;
use crate::rbac::RbacEnforcer;
use crate::storage::ConfigStore;
use crate::xds::SimpleXdsServer;

#[derive(Clone)]
pub struct AppState {
    pub store: ConfigStore,
    pub xds_server: SimpleXdsServer,
    pub jwt_keys: JwtKeys,
    pub rbac: RbacEnforcer,
    pub config: std::sync::Arc<AppConfig>,
}

pub fn create_router(
    store: ConfigStore,
    xds_server: SimpleXdsServer,
    jwt_keys: JwtKeys,
    rbac: RbacEnforcer,
    config: std::sync::Arc<AppConfig>,
) -> Router {
    // Create secure CORS configuration based on application config
    let cors_layer = create_cors_layer(&config).expect("Failed to create CORS configuration");
    let app_state = AppState {
        store,
        xds_server,
        jwt_keys: jwt_keys.clone(),
        rbac: rbac.clone(),
        config: config.clone(),
    };

    // Protected routes that require full authentication & authorization
    let protected_routes = Router::new()
        // Route management (write operations)
        .route("/routes", post(handlers::create_route))
        .route("/routes/{name}", put(handlers::update_route))
        .route("/routes/{name}", delete(handlers::delete_route))
        // Cluster management (write operations)
        .route("/clusters", post(handlers::create_cluster))
        .route("/clusters/{name}", put(handlers::update_cluster))
        .route("/clusters/{name}", delete(handlers::delete_cluster))
        // HTTP Filter management (write operations)
        .route("/http-filters", post(handlers::create_http_filter))
        .route("/http-filters/{name}", put(handlers::update_http_filter))
        .route("/http-filters/{name}", delete(handlers::delete_http_filter))
        // Route-Filter association management (write operations)
        .route("/route-filters", post(handlers::create_route_filters))
        .route("/route-filters/{route_name}", put(handlers::update_route_filters))
        .route("/route-filters/{route_name}", delete(handlers::delete_route_filters))
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
        .route("/routes/{name}", get(handlers::get_route))
        .route("/clusters", get(handlers::list_clusters))
        .route("/clusters/{name}", get(handlers::get_cluster))
        // HTTP Filter read operations
        .route("/http-filters", get(handlers::list_http_filters))
        .route("/http-filters/{name}", get(handlers::get_http_filter))
        // Route-Filter association read operations
        .route("/route-filters/{route_name}", get(handlers::get_route_filters))
        // System info (public)
        .route("/supported-http-methods", get(handlers::get_supported_http_methods))
        .route("/supported-http-filter-types", get(handlers::get_supported_http_filter_types))
        .route("/default-http-filter-order", get(handlers::get_default_http_filter_order))
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
        // Add secure CORS middleware for frontend access with credentials
        .layer(cors_layer)
        // Share the app state across all handlers
        .with_state(app_state)
}

/// Create a secure CORS configuration that works with credentials
/// 
/// This configuration:
/// - Uses specific allowed methods (not Any) to work with credentials
/// - Uses specific allowed headers (not Any) to work with credentials 
/// - Reads HTTP methods from application configuration
/// - Uses minimal required headers for security
/// - Loads origins from environment variables for production security
/// - Follows principle of least privilege for CORS policy
/// 
/// SECURITY CONSIDERATIONS:
/// - When allow_credentials(true) is set, cannot use Any for origins/headers/methods
/// - Only methods actually used by API endpoints are allowed
/// - Origins must be explicitly configured in production via CORS_ALLOWED_ORIGINS
/// - Headers are minimal set required for JSON API + auth + CSRF protection
fn create_cors_layer(config: &AppConfig) -> anyhow::Result<CorsLayer> {
    // Use provided application configuration to get supported HTTP methods
    
    // Convert configured HTTP methods to Axum Method types
    // Only use methods that our API actually supports
    let api_methods: Vec<Method> = config
        .control_plane
        .http_methods
        .supported_methods
        .iter()
        .filter_map(|method_str| {
            // Parse method string and only include methods our API uses
            match method_str.parse::<Method>() {
                Ok(method) => {
                    // Only allow methods that our API endpoints actually use
                    match method {
                        Method::GET | Method::POST | Method::PUT | 
                        Method::DELETE | Method::OPTIONS => Some(method),
                        _ => {
                            // Log unsupported methods for debugging
                            tracing::debug!("Configured method {} not used by API endpoints", method_str);
                            None
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!("Invalid HTTP method in config: {}", method_str);
                    None
                }
            }
        })
        .collect();

    // Ensure OPTIONS is always included for CORS preflight
    let mut final_methods = api_methods;
    if !final_methods.contains(&Method::OPTIONS) {
        final_methods.push(Method::OPTIONS);
    }

    // Define minimal required headers for our application
    // These are the only headers our frontend sends and our API expects
    let allowed_headers = [
        HeaderName::from_static("content-type"),    // Required for JSON API requests
        HeaderName::from_static("authorization"),   // For backward compatibility with Bearer tokens
        HeaderName::from_static("x-requested-with"), // For CSRF protection
        HeaderName::from_static("accept"),          // Standard request header
        HeaderName::from_static("origin"),          // Required for CORS
        HeaderName::from_static("x-csrf-token"),    // Future CSRF token support
        HeaderName::from_static("cache-control"),   // Cache management for API responses
    ];

    // Load allowed origins from environment variables for security
    let allowed_origins = load_cors_origins()?;

    tracing::info!("CORS configuration:");
    tracing::info!("  - Allowed methods: {:?}", final_methods);
    tracing::info!("  - Allowed headers: {:?}", allowed_headers);
    tracing::info!("  - Allowed origins: {:?}", allowed_origins);
    tracing::info!("  - Credentials: enabled (required for httpOnly cookies)");

    let cors_layer = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods(final_methods)
        .allow_headers(allowed_headers)
        .allow_credentials(true); // Essential for httpOnly cookies

    Ok(cors_layer)
}

/// Load CORS allowed origins from environment variables with secure defaults
/// Environment variables:
/// - CORS_ALLOWED_ORIGINS: Comma-separated list of allowed origins
/// - NODE_ENV: If "development", adds default dev origins
fn load_cors_origins() -> anyhow::Result<Vec<axum::http::HeaderValue>> {
    let mut origins = Vec::new();
    
    // Check for explicit CORS origins from environment
    if let Ok(cors_origins) = std::env::var("CORS_ALLOWED_ORIGINS") {
        for origin in cors_origins.split(',') {
            let origin = origin.trim();
            if !origin.is_empty() {
                match origin.parse::<axum::http::HeaderValue>() {
                    Ok(header_value) => {
                        origins.push(header_value);
                        tracing::info!("Added CORS origin from environment: {}", origin);
                    }
                    Err(e) => {
                        tracing::warn!("Invalid CORS origin '{}': {}", origin, e);
                    }
                }
            }
        }
    }
    
    // Add development origins if in development mode or if no explicit origins set
    let is_development = std::env::var("NODE_ENV").unwrap_or_default() == "development" || 
                        std::env::var("RUST_ENV").unwrap_or_default() == "development";
    
    if origins.is_empty() || is_development {
        // Default development origins
        let dev_origins = [
            "http://localhost:5173",    // Vite dev server
            "http://127.0.0.1:5173",    // Alternative localhost
            "http://localhost:3000",    // Alternative React dev server
            "http://127.0.0.1:3000",    // Alternative localhost
        ];
        
        for origin in dev_origins {
            match origin.parse::<axum::http::HeaderValue>() {
                Ok(header_value) => {
                    origins.push(header_value);
                    if is_development {
                        tracing::info!("Added development CORS origin: {}", origin);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to parse default origin '{}': {}", origin, e);
                }
            }
        }
        
        if !is_development {
            tracing::warn!("No CORS_ALLOWED_ORIGINS set - using development defaults. Set CORS_ALLOWED_ORIGINS for production.");
        }
    }
    
    if origins.is_empty() {
        return Err(anyhow::anyhow!("No valid CORS origins configured"));
    }
    
    Ok(origins)
}

async fn health_check() -> &'static str {
    "OK"
}
