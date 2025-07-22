use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::api::routes::AppState;
use crate::config::AppConfig;
use crate::envoy::ConfigGenerator;
use crate::storage::{Cluster, Endpoint, Route, LoadBalancingPolicy};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateClusterRequest {
    pub name: String,
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
    Json(payload): Json<CreateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let route = Route::new(payload.path, payload.cluster_name, payload.prefix_rewrite);
    let id = app_state.store.add_route(route);

    // Increment version to notify Envoy of the change
    app_state.xds_server.increment_version();

    Ok(Json(ApiResponse::success(id, "Route created successfully")))
}

pub async fn get_route(
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Route>>, StatusCode> {
    match app_state.store.get_route(&id) {
        Some(route) => Ok(Json(ApiResponse::success(route, "Route found"))),
        None => Err(StatusCode::NOT_FOUND),
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
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match app_state.store.remove_route(&id) {
        Some(_) => {
            // Increment version to notify Envoy of the deletion
            app_state.xds_server.increment_version();
            Ok(Json(ApiResponse::success((), "Route deleted successfully")))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

// Cluster handlers
pub async fn create_cluster(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Load config to validate lb_policy
    let config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

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
            if !config.load_balancing.available_policies.contains(&policy_str) {
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!(
                        "Invalid load balancing policy '{}'. Available policies: {:?}",
                        policy_str, config.load_balancing.available_policies
                    ),
                }));
            }
            
            // Convert string to enum and create cluster
            let lb_policy = LoadBalancingPolicy::from_str(&policy_str);
            Cluster::with_lb_policy(payload.name, endpoints, lb_policy)
        }
        None => {
            // No policy specified - use default from config
            let default_policy = LoadBalancingPolicy::from_str(&config.load_balancing.default_policy);
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
) -> Result<Json<ApiResponse<Cluster>>, StatusCode> {
    match app_state.store.get_cluster(&name) {
        Some(cluster) => Ok(Json(ApiResponse::success(cluster, "Cluster found"))),
        None => Err(StatusCode::NOT_FOUND),
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
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match app_state.store.remove_cluster(&name) {
        Some(_) => {
            // Increment version to notify Envoy of the deletion
            app_state.xds_server.increment_version();
            Ok(Json(ApiResponse::success(
                (),
                "Cluster deleted successfully",
            )))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateConfigRequest {
    pub proxy_name: String,
    pub proxy_port: u16,
}

pub async fn generate_envoy_config(
    State(app_state): State<AppState>,
    Json(payload): Json<GenerateConfigRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Load app config (in a real app, this would be injected as state)
    let app_config = match AppConfig::load() {
        Ok(config) => config,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Generate Envoy configuration
    let envoy_config =
        match ConfigGenerator::generate_config(&app_state.store, &app_config, payload.proxy_port) {
            Ok(config) => config,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

    // Write to file
    let config_dir = &app_config.envoy.config_dir;
    let file_path = config_dir.join(format!("{}.yaml", payload.proxy_name));

    // Ensure config directory exists
    if std::fs::create_dir_all(config_dir).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    match ConfigGenerator::write_config_to_file(&envoy_config, &file_path) {
        Ok(_) => {
            let file_path_str = file_path.to_string_lossy().to_string();
            Ok(Json(ApiResponse::success(
                file_path_str,
                "Envoy configuration generated successfully",
            )))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
