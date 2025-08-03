use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::api::errors::ApiError;
use crate::api::routes::AppState;
use crate::config::AppConfig;
use crate::envoy::ConfigGenerator;
use crate::storage::{Cluster, Endpoint, Route, LoadBalancingPolicy};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
    pub http_methods: Option<Vec<String>>, // GET, POST, PUT, DELETE, etc.
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateClusterRequest {
    pub name: String,
    pub endpoints: Vec<CreateEndpointRequest>,
    pub lb_policy: Option<String>, // Optional: will use config default if None
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateClusterRequest {
    pub endpoints: Vec<CreateEndpointRequest>,
    pub lb_policy: Option<String>, // Optional: will use config default if None
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRouteRequest {
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
    pub http_methods: Option<Vec<String>>, // GET, POST, PUT, DELETE, etc.
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateEndpointRequest {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: message.to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn error(message: &str) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            message: message.to_string(),
        }
    }
}

// Route handlers
pub async fn create_route(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Load config to validate HTTP methods
    let config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
    };

    // Validate HTTP methods if provided
    if let Some(ref methods) = payload.http_methods {
        for method in methods {
            if !is_valid_http_method(method, &config.control_plane.http_methods.supported_methods) {
                return Err(ApiError::validation(format!(
                    "Invalid HTTP method '{}'. Supported methods: {:?}", 
                    method, 
                    config.control_plane.http_methods.supported_methods
                )));
            }
        }
    }

    let route = Route::with_methods(
        payload.path, 
        payload.cluster_name, 
        payload.prefix_rewrite,
        payload.http_methods
    );
    let id = app_state.store.add_route(route);

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(id, "Route created successfully")))
}

pub async fn update_route(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Check if route exists
    if app_state.store.get_route(&id).is_none() {
        return Err(ApiError::not_found("route".to_string()));
    }

    // Load config to validate HTTP methods
    let config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
    };

    // Validate HTTP methods if provided
    if let Some(ref methods) = payload.http_methods {
        for method in methods {
            if !is_valid_http_method(method, &config.control_plane.http_methods.supported_methods) {
                return Err(ApiError::validation(format!(
                    "Invalid HTTP method '{}'. Supported methods: {:?}", 
                    method, 
                    config.control_plane.http_methods.supported_methods
                )));
            }
        }
    }

    // Create updated route with the same ID
    let updated_route = Route {
        id: id.clone(),
        path: payload.path,
        cluster_name: payload.cluster_name,
        prefix_rewrite: payload.prefix_rewrite,
        http_methods: payload.http_methods,
    };

    match app_state.store.update_route(&id, updated_route) {
        Some(_) => {
            // Increment version to notify Envoy of the change
            app_state.xds_server.increment_version();
            Ok(Json(ApiResponse::success(id, "Route updated successfully")))
        }
        None => Err(ApiError::not_found("resource".to_string())),
    }
}

pub async fn get_route(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Route>>, ApiError> {
    match app_state.store.get_route(&id) {
        Some(route) => Ok(Json(ApiResponse::success(route, "Route found"))),
        None => Err(ApiError::not_found("resource".to_string())),
    }
}

pub async fn list_routes(State(app_state): State<AppState>) -> Json<ApiResponse<Vec<Route>>> {
    let routes = app_state.store.list_routes();
    Json(ApiResponse::success(
        routes,
        "Routes retrieved successfully",
    ))
}

pub async fn delete_route(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    match app_state.store.remove_route(&id) {
        Some(_) => {
            // Increment version to notify Envoy of the deletion
            app_state.xds_server.increment_version();
            Ok(Json(ApiResponse::success((), "Route deleted successfully")))
        }
        None => Err(ApiError::not_found("resource".to_string())),
    }
}

// Cluster handlers
pub async fn create_cluster(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Load config to validate lb_policy
    let config = AppConfig::load()
        .map_err(|e| ApiError::configuration(format!("Failed to load application configuration: {}", e)))?;

    // Convert endpoints
    let endpoints: Vec<Endpoint> = payload
        .endpoints
        .into_iter()
        .map(|e| Endpoint::new(e.host, e.port))
        .collect();

    // Handle load balancing policy
    let cluster = match payload.lb_policy {
        Some(policy_str) => {
            // Validate policy against available_policies registry
            if !config
                .control_plane
                .load_balancing
                .available_policies
                .contains(&policy_str)
            {
                return Err(ApiError::validation(format!(
                    "Invalid load balancing policy '{}'. Available policies: {:?}",
                    policy_str, config.control_plane.load_balancing.available_policies
                )));
            }

            // Convert string to enum and create cluster
            let lb_policy = LoadBalancingPolicy::parse_safe(&policy_str, "create_cluster request")
                .map_err(ApiError::parse)?;
            Cluster::with_lb_policy(payload.name, endpoints, lb_policy)
        }
        None => {
            // No policy specified - use default from config
            let default_policy = LoadBalancingPolicy::parse_safe(
                &config.control_plane.load_balancing.default_policy,
                "default load balancing policy from config"
            ).map_err(ApiError::configuration)?;
            Cluster::with_lb_policy(payload.name, endpoints, default_policy)
        }
    };

    let name = app_state.store.add_cluster(cluster);

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(
        name,
        "Cluster created successfully",
    )))
}

pub async fn get_cluster(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<Cluster>>, ApiError> {
    match app_state.store.get_cluster(&name) {
        Some(cluster) => Ok(Json(ApiResponse::success(cluster, "Cluster found"))),
        None => Err(ApiError::not_found("resource".to_string())),
    }
}

pub async fn list_clusters(State(app_state): State<AppState>) -> Json<ApiResponse<Vec<Cluster>>> {
    let clusters = app_state.store.list_clusters();
    Json(ApiResponse::success(
        clusters,
        "Clusters retrieved successfully",
    ))
}

pub async fn delete_cluster(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    match app_state.store.remove_cluster(&name) {
        Some(_) => {
            // Increment version to notify Envoy of the deletion
            app_state.xds_server.increment_version();
            Ok(Json(ApiResponse::success(
                (),
                "Cluster deleted successfully",
            )))
        }
        None => Err(ApiError::not_found("resource".to_string())),
    }
}

pub async fn update_cluster(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<UpdateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Check if cluster exists
    if app_state.store.get_cluster(&name).is_none() {
        return Err(ApiError::not_found(format!("cluster '{}'", name)));
    }

    // Load config to validate lb_policy
    let config = AppConfig::load()
        .map_err(|e| ApiError::configuration(format!("Failed to load application configuration: {}", e)))?;

    // Convert endpoints
    let endpoints: Vec<Endpoint> = payload
        .endpoints
        .into_iter()
        .map(|e| Endpoint::new(e.host, e.port))
        .collect();

    // Handle load balancing policy
    let cluster = match payload.lb_policy {
        Some(policy_str) => {
            // Validate policy against available_policies registry
            if !config
                .control_plane
                .load_balancing
                .available_policies
                .contains(&policy_str)
            {
                return Err(ApiError::validation(format!(
                    "Invalid load balancing policy '{}'. Available policies: {:?}",
                    policy_str, config.control_plane.load_balancing.available_policies
                )));
            }

            // Convert string to enum and create cluster
            let lb_policy = LoadBalancingPolicy::parse_safe(&policy_str, "update_cluster request")
                .map_err(ApiError::parse)?;
            Cluster::with_lb_policy(name.clone(), endpoints, lb_policy)
        }
        None => {
            // No policy specified - use default from config
            let default_policy = LoadBalancingPolicy::parse_safe(
                &config.control_plane.load_balancing.default_policy,
                "default load balancing policy from config"
            ).map_err(ApiError::configuration)?;
            Cluster::with_lb_policy(name.clone(), endpoints, default_policy)
        }
    };

    // Update the cluster (remove old, add new)
    app_state.store.remove_cluster(&name);
    let updated_name = app_state.store.add_cluster(cluster);

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(
        updated_name,
        "Cluster updated successfully",
    )))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateConfigRequest {
    pub proxy_name: String,
    pub proxy_port: u16,
}

pub async fn get_supported_http_methods(
    State(_app_state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<String>>>, ApiError> {
    // Load app config
    let app_config = match AppConfig::load() {
        Ok(config) => config,
        Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
    };

    Ok(Json(ApiResponse::success(
        app_config.control_plane.http_methods.supported_methods,
        "Supported HTTP methods retrieved successfully",
    )))
}

pub async fn generate_bootstrap_config(
    State(_app_state): State<AppState>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Load app config
    let app_config = match AppConfig::load() {
        Ok(config) => config,
        Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
    };

    // Generate bootstrap configuration
    match ConfigGenerator::generate_bootstrap_config(&app_config) {
        Ok(bootstrap_yaml) => Ok(Json(ApiResponse::success(
            bootstrap_yaml,
            "Bootstrap configuration generated successfully",
        ))),
        Err(e) => Err(ApiError::internal(format!("Operation failed: {}", e))),
    }
}

pub async fn generate_envoy_config(
    State(app_state): State<AppState>,
    Json(payload): Json<GenerateConfigRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Load app config (in a real app, this would be injected as state)
    let app_config = match AppConfig::load() {
        Ok(config) => config,
        Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
    };

    // Generate Envoy configuration
    let envoy_config =
        match ConfigGenerator::generate_config(&app_state.store, &app_config, payload.proxy_port) {
            Ok(config) => config,
            Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
        };

    // Write to file
    let config_dir = &app_config.envoy_generation.config_dir;
    let file_path = config_dir.join(format!("{}.yaml", payload.proxy_name));

    // Ensure config directory exists
    if std::fs::create_dir_all(config_dir).is_err() {
        return Err(ApiError::internal("Failed to create configuration directory".to_string()));
    }

    match ConfigGenerator::write_config_to_file(&envoy_config, &file_path) {
        Ok(_) => {
            let file_path_str = file_path.to_string_lossy().to_string();
            Ok(Json(ApiResponse::success(
                file_path_str,
                "Envoy configuration generated successfully",
            )))
        }
        Err(e) => Err(ApiError::internal(format!("Operation failed: {}", e))),
    }
}

// Helper function to validate HTTP methods against config
fn is_valid_http_method(method: &str, supported_methods: &[String]) -> bool {
    supported_methods.iter().any(|m| m.eq_ignore_ascii_case(method))
}
