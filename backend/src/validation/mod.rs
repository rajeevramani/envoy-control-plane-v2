use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

/// Validation patterns for different input types
lazy_static! {
    /// Route names: alphanumeric, underscore, hyphen only (1-100 chars)
    static ref ROUTE_NAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    
    /// Cluster names: alphanumeric, underscore, period, hyphen only (1-50 chars)
    static ref CLUSTER_NAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap();
    
    /// Host validation: alphanumeric, period, hyphen (for domains and IPs)
    static ref HOST_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9.-]+$").unwrap();
    
    /// Path validation: starts with /, contains safe URL characters
    static ref PATH_REGEX: Regex = Regex::new(r"^/[a-zA-Z0-9/_.-]*$").unwrap();
    
    /// HTTP method validation: standard HTTP verbs only
    static ref HTTP_METHOD_REGEX: Regex = Regex::new(r"^(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS|TRACE|CONNECT)$").unwrap();
    
    /// Load balancing policy validation
    static ref LB_POLICY_REGEX: Regex = Regex::new(r"^(ROUND_ROBIN|LEAST_REQUEST|RANDOM|RING_HASH)$").unwrap();
}

/// Custom validation functions
pub fn validate_route_name(name: &str) -> Result<(), ValidationError> {
    if !ROUTE_NAME_REGEX.is_match(name) {
        return Err(ValidationError::new("invalid_route_name"));
    }
    Ok(())
}

pub fn validate_cluster_name(name: &str) -> Result<(), ValidationError> {
    if !CLUSTER_NAME_REGEX.is_match(name) {
        return Err(ValidationError::new("invalid_cluster_name"));
    }
    Ok(())
}

pub fn validate_host(host: &str) -> Result<(), ValidationError> {
    if !HOST_REGEX.is_match(host) {
        return Err(ValidationError::new("invalid_host"));
    }
    Ok(())
}

pub fn validate_path(path: &str) -> Result<(), ValidationError> {
    // Check for path traversal attempts
    if path.contains("..") || path.contains("//") {
        return Err(ValidationError::new("path_traversal_detected"));
    }
    
    if !PATH_REGEX.is_match(path) {
        return Err(ValidationError::new("invalid_path_format"));
    }
    Ok(())
}

pub fn validate_http_method(method: &str) -> Result<(), ValidationError> {
    if !HTTP_METHOD_REGEX.is_match(method) {
        return Err(ValidationError::new("invalid_http_method"));
    }
    Ok(())
}

pub fn validate_lb_policy(policy: &str) -> Result<(), ValidationError> {
    if !LB_POLICY_REGEX.is_match(policy) {
        return Err(ValidationError::new("invalid_lb_policy"));
    }
    Ok(())
}

/// Validation helper for HTTP methods list
pub fn validate_http_methods(methods: &Vec<String>) -> Result<(), ValidationError> {
    if methods.is_empty() {
        return Err(ValidationError::new("empty_http_methods"));
    }
    
    if methods.len() > 10 {
        return Err(ValidationError::new("too_many_http_methods"));
    }
    
    for method in methods {
        validate_http_method(method)?;
    }
    Ok(())
}

/// Validated request structures with derive-based validation

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ValidatedCreateRouteRequest {
    #[validate(length(min = 1, max = 100), custom(function = "validate_route_name"))]
    pub name: String,
    
    #[validate(length(min = 1, max = 200), custom(function = "validate_path"))]
    pub path: String,
    
    #[validate(length(min = 1, max = 50), custom(function = "validate_cluster_name"))]
    pub cluster_name: String,
    
    #[validate(length(max = 100))]
    pub prefix_rewrite: Option<String>,
    
    #[validate(custom(function = "validate_http_methods"))]
    pub http_methods: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ValidatedUpdateRouteRequest {
    #[validate(length(min = 1, max = 200), custom(function = "validate_path"))]
    pub path: String,
    
    #[validate(length(min = 1, max = 50), custom(function = "validate_cluster_name"))]
    pub cluster_name: String,
    
    #[validate(length(max = 100))]
    pub prefix_rewrite: Option<String>,
    
    #[validate(custom(function = "validate_http_methods"))]
    pub http_methods: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ValidatedCreateClusterRequest {
    #[validate(length(min = 1, max = 50), custom(function = "validate_cluster_name"))]
    pub name: String,
    
    #[validate(length(min = 1, max = 10))]
    pub endpoints: Vec<ValidatedCreateEndpointRequest>,
    
    #[validate(custom(function = "validate_lb_policy"))]
    pub lb_policy: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ValidatedUpdateClusterRequest {
    #[validate(length(min = 1, max = 10))]
    pub endpoints: Vec<ValidatedCreateEndpointRequest>,
    
    #[validate(custom(function = "validate_lb_policy"))]
    pub lb_policy: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ValidatedCreateEndpointRequest {
    #[validate(length(min = 1, max = 255), custom(function = "validate_host"))]
    pub host: String,
    
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,
}

/// Conversion functions from validated to internal types
impl From<ValidatedCreateRouteRequest> for crate::api::handlers::CreateRouteRequest {
    fn from(validated: ValidatedCreateRouteRequest) -> Self {
        Self {
            name: validated.name,
            path: validated.path,
            cluster_name: validated.cluster_name,
            prefix_rewrite: validated.prefix_rewrite,
            http_methods: validated.http_methods,
        }
    }
}

impl From<ValidatedUpdateRouteRequest> for crate::api::handlers::UpdateRouteRequest {
    fn from(validated: ValidatedUpdateRouteRequest) -> Self {
        Self {
            path: validated.path,
            cluster_name: validated.cluster_name,
            prefix_rewrite: validated.prefix_rewrite,
            http_methods: validated.http_methods,
        }
    }
}

impl From<ValidatedCreateClusterRequest> for crate::api::handlers::CreateClusterRequest {
    fn from(validated: ValidatedCreateClusterRequest) -> Self {
        Self {
            name: validated.name,
            endpoints: validated.endpoints.into_iter().map(Into::into).collect(),
            lb_policy: validated.lb_policy,
        }
    }
}

impl From<ValidatedUpdateClusterRequest> for crate::api::handlers::UpdateClusterRequest {
    fn from(validated: ValidatedUpdateClusterRequest) -> Self {
        Self {
            endpoints: validated.endpoints.into_iter().map(Into::into).collect(),
            lb_policy: validated.lb_policy,
        }
    }
}

impl From<ValidatedCreateEndpointRequest> for crate::api::handlers::CreateEndpointRequest {
    fn from(validated: ValidatedCreateEndpointRequest) -> Self {
        Self {
            host: validated.host,
            port: validated.port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_name_validation() {
        assert!(validate_route_name("valid-route_123").is_ok());
        assert!(validate_route_name("invalid/route").is_err());
        assert!(validate_route_name("invalid route").is_err());
    }

    #[test]
    fn test_cluster_name_validation() {
        assert!(validate_cluster_name("valid-cluster.name_123").is_ok());
        assert!(validate_cluster_name("invalid/cluster").is_err());
        assert!(validate_cluster_name("invalid cluster").is_err());
    }

    #[test]
    fn test_path_validation() {
        assert!(validate_path("/api/v1/users").is_ok());
        assert!(validate_path("/api/../etc/passwd").is_err());
        assert!(validate_path("//invalid").is_err());
        assert!(validate_path("invalid").is_err()); // Must start with /
    }

    #[test]
    fn test_host_validation() {
        assert!(validate_host("example.com").is_ok());
        assert!(validate_host("192.168.1.1").is_ok());
        assert!(validate_host("localhost").is_ok());
        assert!(validate_host("invalid host").is_err());
        assert!(validate_host("invalid/host").is_err());
    }

    #[test]
    fn test_http_method_validation() {
        assert!(validate_http_method("GET").is_ok());
        assert!(validate_http_method("POST").is_ok());
        assert!(validate_http_method("INVALID").is_err());
        assert!(validate_http_method("get").is_err()); // Case sensitive  
    }

    #[test]
    fn test_lb_policy_validation() {
        assert!(validate_lb_policy("ROUND_ROBIN").is_ok());
        assert!(validate_lb_policy("LEAST_REQUEST").is_ok());
        assert!(validate_lb_policy("INVALID_POLICY").is_err());
    }
}