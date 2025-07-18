use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::storage::{ConfigStore, Route, Cluster, Endpoint};
use crate::envoy::ConfigGenerator;
use crate::config::AppConfig;

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
    State(store): State<ConfigStore>,
    Json(payload): Json<CreateRouteRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let route = Route::new(payload.path, payload.cluster_name, payload.prefix_rewrite);
    let id = store.add_route(route);
    
    Ok(Json(ApiResponse::success(id, "Route created successfully")))
}

pub async fn get_route(
    State(store): State<ConfigStore>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Route>>, StatusCode> {
    match store.get_route(&id) {
        Some(route) => Ok(Json(ApiResponse::success(route, "Route found"))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn list_routes(
    State(store): State<ConfigStore>,
) -> Json<ApiResponse<Vec<Route>>> {
    let routes = store.list_routes();
    Json(ApiResponse::success(routes, "Routes retrieved successfully"))
}

pub async fn delete_route(
    State(store): State<ConfigStore>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match store.remove_route(&id) {
        Some(_) => Ok(Json(ApiResponse::success((), "Route deleted successfully"))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

// Cluster handlers
pub async fn create_cluster(
    State(store): State<ConfigStore>,
    Json(payload): Json<CreateClusterRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let endpoints: Vec<Endpoint> = payload.endpoints
        .into_iter()
        .map(|e| Endpoint::new(e.host, e.port))
        .collect();
    
    let cluster = Cluster::new(payload.name, endpoints);
    let name = store.add_cluster(cluster);
    
    Ok(Json(ApiResponse::success(name, "Cluster created successfully")))
}

pub async fn get_cluster(
    State(store): State<ConfigStore>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<Cluster>>, StatusCode> {
    match store.get_cluster(&name) {
        Some(cluster) => Ok(Json(ApiResponse::success(cluster, "Cluster found"))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn list_clusters(
    State(store): State<ConfigStore>,
) -> Json<ApiResponse<Vec<Cluster>>> {
    let clusters = store.list_clusters();
    Json(ApiResponse::success(clusters, "Clusters retrieved successfully"))
}

pub async fn delete_cluster(
    State(store): State<ConfigStore>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match store.remove_cluster(&name) {
        Some(_) => Ok(Json(ApiResponse::success((), "Cluster deleted successfully"))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateConfigRequest {
    pub proxy_name: String,
    pub proxy_port: u16,
}

pub async fn generate_envoy_config(
    State(store): State<ConfigStore>,
    Json(payload): Json<GenerateConfigRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Load app config (in a real app, this would be injected as state)
    let app_config = match AppConfig::load() {
        Ok(config) => config,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Generate Envoy configuration
    let envoy_config = match ConfigGenerator::generate_config(&store, &app_config, payload.proxy_port) {
        Ok(config) => config,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Write to file
    let config_dir = &app_config.envoy.config_dir;
    let file_path = config_dir.join(format!("{}.yaml", payload.proxy_name));
    
    // Ensure config directory exists
    if let Err(_) = std::fs::create_dir_all(config_dir) {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    match ConfigGenerator::write_config_to_file(&envoy_config, &file_path) {
        Ok(_) => {
            let file_path_str = file_path.to_string_lossy().to_string();
            Ok(Json(ApiResponse::success(
                file_path_str,
                "Envoy configuration generated successfully"
            )))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}