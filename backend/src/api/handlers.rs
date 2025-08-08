use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::api::errors::ApiError;
use crate::api::routes::AppState;
use crate::config::AppConfig;
use crate::envoy::ConfigGenerator;
use crate::storage::{Cluster, Endpoint, Route, LoadBalancingPolicy};
use crate::validation::{
    ValidatedCreateRouteRequest, ValidatedUpdateRouteRequest,
    ValidatedCreateClusterRequest, ValidatedUpdateClusterRequest,
};


#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    pub name: String,
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
    pub http_methods: Option<Vec<String>>, // GET, POST, PUT, DELETE, etc.
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRouteRequest {
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
    Json(payload): Json<ValidatedCreateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Validate the input
    payload.validate()?;
    
    // Convert to internal type
    let payload: CreateRouteRequest = payload.into();
    
    // Check for duplicate route names - if get_route succeeds, route already exists
    if app_state.store.get_route(&payload.name).is_ok() {
        return Err(ApiError::validation(format!(
            "Route with name '{}' already exists", 
            payload.name
        )));
    }

    let route = Route::with_methods(
        payload.name.clone(),
        payload.path, 
        payload.cluster_name, 
        payload.prefix_rewrite,
        payload.http_methods
    );
    let name = app_state.store.add_route(route)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(name, "Route created successfully")))
}

pub async fn update_route(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<ValidatedUpdateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Validate the input
    payload.validate()?;
    
    // Convert to internal type
    let payload: UpdateRouteRequest = payload.into();
    
    // Create updated route with the same ID
    let updated_route = Route {
        name: name.clone(),
        path: payload.path,
        cluster_name: payload.cluster_name,
        prefix_rewrite: payload.prefix_rewrite,
        http_methods: payload.http_methods,
    };

    // update_route will return StorageError if route doesn't exist
    app_state.store.update_route(&name, updated_route)?;
    
    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();
    Ok(Json(ApiResponse::success(name, "Route updated successfully")))
}

pub async fn get_route(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<Route>>, ApiError> {
    let route = app_state.store.get_route(&name)?;
    Ok(Json(ApiResponse::success(route, "Route found")))
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
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    app_state.store.remove_route(&name)?;
    
    // Increment version to notify Envoy of the deletion
    app_state.xds_server.increment_version();
    Ok(Json(ApiResponse::success((), "Route deleted successfully")))
}

// Cluster handlers
pub async fn create_cluster(
    State(app_state): State<AppState>,
    Json(payload): Json<ValidatedCreateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Validate the input
    payload.validate()?;
    
    // Convert to internal type
    let payload: CreateClusterRequest = payload.into();

    // Convert endpoints
    let endpoints: Vec<Endpoint> = payload
        .endpoints
        .into_iter()
        .map(|e| Endpoint::new(e.host, e.port))
        .collect();

    // Handle load balancing policy - validation already done in validation layer
    let cluster = match payload.lb_policy {
        Some(policy_str) => {
            let lb_policy = policy_str.parse::<LoadBalancingPolicy>()
                .map_err(|_| ApiError::validation(format!("Invalid load balancing policy: {}", policy_str)))?;
            Cluster::with_lb_policy(payload.name, endpoints, lb_policy)
        }
        None => {
            // No policy specified - use cluster with no specific policy (will use system default)
            Cluster::new(payload.name, endpoints)
        }
    };

    let name = app_state.store.add_cluster(cluster)?;

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
    let cluster = app_state.store.get_cluster(&name)?;
    Ok(Json(ApiResponse::success(cluster, "Cluster found")))
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
    app_state.store.remove_cluster(&name)?;
    
    // Increment version to notify Envoy of the deletion
    app_state.xds_server.increment_version();
    Ok(Json(ApiResponse::success(
        (),
        "Cluster deleted successfully",
    )))
}

pub async fn update_cluster(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<ValidatedUpdateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Validate the input
    payload.validate()?;
    
    // Check if cluster exists - get_cluster will return StorageError if not found
    app_state.store.get_cluster(&name)?;
    
    // Convert to internal type
    let payload: UpdateClusterRequest = payload.into();

    // Convert endpoints
    let endpoints: Vec<Endpoint> = payload
        .endpoints
        .into_iter()
        .map(|e| Endpoint::new(e.host, e.port))
        .collect();

    // Handle load balancing policy - validation already done in validation layer
    let cluster = match payload.lb_policy {
        Some(policy_str) => {
            let lb_policy = policy_str.parse::<LoadBalancingPolicy>()
                .map_err(|_| ApiError::validation(format!("Invalid load balancing policy: {}", policy_str)))?;
            Cluster::with_lb_policy(name.clone(), endpoints, lb_policy)
        }
        None => {
            // No policy specified - use cluster with no specific policy (will use system default)
            Cluster::new(name.clone(), endpoints)
        }
    };

    // Update the cluster using the new update_cluster method
    app_state.store.update_cluster(&name, cluster)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(
        name,
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
