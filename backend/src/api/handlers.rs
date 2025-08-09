use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::api::errors::ApiError;
use crate::api::routes::AppState;
use crate::envoy::ConfigGenerator;
use crate::storage::{Cluster, Endpoint, Route, LoadBalancingPolicy, HttpFilter, RouteFilters};
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
    Ok(Json(ApiResponse::success((*route).clone(), "Route found")))
}

pub async fn list_routes(State(app_state): State<AppState>) -> Json<ApiResponse<Vec<Route>>> {
    let routes = app_state.store.list_routes();
    Json(ApiResponse::success(
        routes.iter().map(|r| (**r).clone()).collect(),
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
    Ok(Json(ApiResponse::success((*cluster).clone(), "Cluster found")))
}

pub async fn list_clusters(State(app_state): State<AppState>) -> Json<ApiResponse<Vec<Cluster>>> {
    let clusters = app_state.store.list_clusters();
    Json(ApiResponse::success(
        clusters.iter().map(|c| (**c).clone()).collect(),
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
    State(app_state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<String>>>, ApiError> {
    // Use shared app config from state (no file I/O!)
    Ok(Json(ApiResponse::success(
        app_state.config.control_plane.http_methods.supported_methods.clone(),
        "Supported HTTP methods retrieved successfully",
    )))
}

pub async fn generate_bootstrap_config(
    State(app_state): State<AppState>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Use shared app config from state (no file I/O!)
    // Generate bootstrap configuration
    match ConfigGenerator::generate_bootstrap_config(&app_state.config) {
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
    // Use shared app config from state (no file I/O!)

    // Generate Envoy configuration
    let envoy_config =
        match ConfigGenerator::generate_config(&app_state.store, &app_state.config, payload.proxy_port) {
            Ok(config) => config,
            Err(e) => return Err(ApiError::configuration(format!("Failed to load application configuration: {}", e))),
        };

    // Write to file
    let config_dir = &app_state.config.envoy_generation.config_dir;
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

// HTTP Filter request/response structures
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateHttpFilterRequest {
    pub name: String,
    pub filter_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateHttpFilterRequest {
    pub filter_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRouteFiltersRequest {
    pub route_name: String,
    pub filter_names: Vec<String>,
    pub custom_order: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRouteFiltersRequest {
    pub filter_names: Vec<String>,
    pub custom_order: Option<Vec<String>>,
}

// HTTP Filter handlers
pub async fn create_http_filter(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateHttpFilterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Get supported filters from config
    let supported_filters = &app_state.config.control_plane.http_filters.supported_filters;
    
    let filter = HttpFilter::new(
        payload.name.clone(),
        payload.filter_type,
        payload.config,
    ).with_enabled(payload.enabled.unwrap_or(true));

    let name = app_state.store.add_http_filter(filter, supported_filters)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(name, "HTTP filter created successfully")))
}

pub async fn get_http_filter(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<HttpFilter>>, ApiError> {
    let filter = app_state.store.get_http_filter(&name)?;
    Ok(Json(ApiResponse::success((*filter).clone(), "HTTP filter found")))
}

pub async fn list_http_filters(State(app_state): State<AppState>) -> Json<ApiResponse<Vec<HttpFilter>>> {
    let filters = app_state.store.list_http_filters();
    Json(ApiResponse::success(
        filters.iter().map(|f| (**f).clone()).collect(),
        "HTTP filters retrieved successfully",
    ))
}

pub async fn update_http_filter(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<UpdateHttpFilterRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    // Get supported filters from config
    let supported_filters = &app_state.config.control_plane.http_filters.supported_filters;
    
    let updated_filter = HttpFilter::new(
        name.clone(),
        payload.filter_type,
        payload.config,
    ).with_enabled(payload.enabled.unwrap_or(true));

    app_state.store.update_http_filter(&name, updated_filter, supported_filters)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(name, "HTTP filter updated successfully")))
}

pub async fn delete_http_filter(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    app_state.store.remove_http_filter(&name)?;
    
    // Increment version to notify Envoy of the deletion
    app_state.xds_server.increment_version();
    Ok(Json(ApiResponse::success((), "HTTP filter deleted successfully")))
}

// Route-Filter association handlers
pub async fn create_route_filters(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateRouteFiltersRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    let route_filters = RouteFilters::new(
        payload.route_name.clone(),
        payload.filter_names,
    ).with_custom_order(payload.custom_order.unwrap_or_default());

    let route_name = app_state.store.add_route_filters(route_filters)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(route_name, "Route filters created successfully")))
}

pub async fn get_route_filters(
    State(app_state): State<AppState>,
    Path(route_name): Path<String>,
) -> Result<Json<ApiResponse<RouteFilters>>, ApiError> {
    let route_filters = app_state.store.get_route_filters(&route_name).ok_or_else(|| {
        ApiError::not_found(format!("Route filters for '{}' not found", route_name))
    })?;
    Ok(Json(ApiResponse::success(route_filters, "Route filters found")))
}

pub async fn update_route_filters(
    State(app_state): State<AppState>,
    Path(route_name): Path<String>,
    Json(payload): Json<UpdateRouteFiltersRequest>,
) -> Result<Json<ApiResponse<String>>, ApiError> {
    let updated_route_filters = RouteFilters::new(
        route_name.clone(),
        payload.filter_names,
    ).with_custom_order(payload.custom_order.unwrap_or_default());

    app_state.store.remove_route_filters(&route_name)?;
    app_state.store.add_route_filters(updated_route_filters)?;

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(route_name, "Route filters updated successfully")))
}

pub async fn delete_route_filters(
    State(app_state): State<AppState>,
    Path(route_name): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    app_state.store.remove_route_filters(&route_name)?;
    
    // Increment version to notify Envoy of the deletion
    app_state.xds_server.increment_version();
    Ok(Json(ApiResponse::success((), "Route filters deleted successfully")))
}

// Get supported HTTP filter types from config
pub async fn get_supported_http_filter_types(
    State(app_state): State<AppState>,
) -> Json<ApiResponse<Vec<String>>> {
    Json(ApiResponse::success(
        app_state.config.control_plane.http_filters.supported_filters.clone(),
        "Supported HTTP filter types retrieved successfully",
    ))
}

// Get default HTTP filter order from config
pub async fn get_default_http_filter_order(
    State(app_state): State<AppState>,
) -> Json<ApiResponse<Vec<String>>> {
    Json(ApiResponse::success(
        app_state.config.control_plane.http_filters.default_order.clone(),
        "Default HTTP filter order retrieved successfully",
    ))
}
