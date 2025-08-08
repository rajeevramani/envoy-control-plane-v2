use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{error, debug};
use uuid::Uuid;
use validator::ValidationErrors;

/// Request context for error correlation and debugging
#[derive(Debug, Clone, Serialize)]
pub struct RequestContext {
    pub correlation_id: String,
    pub method: String,
    pub path: String,
    pub user_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub client_ip: Option<std::net::IpAddr>,
}

impl RequestContext {
    pub fn new(method: String, path: String) -> Self {
        Self {
            correlation_id: Uuid::new_v4().to_string(),
            method,
            path,
            user_id: None,
            timestamp: chrono::Utc::now(),
            client_ip: None,
        }
    }
    
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }
    
    pub fn with_client_ip(mut self, client_ip: std::net::IpAddr) -> Self {
        self.client_ip = Some(client_ip);
        self
    }
}

/// Enhanced error context for debugging and tracing
#[derive(Debug, Clone, Serialize, Default)]
pub struct ErrorContext {
    pub request: Option<RequestContext>,
    pub operation: Option<String>,
    pub resource: Option<String>,
    pub component: Option<String>,
    pub additional: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_request(mut self, request: RequestContext) -> Self {
        self.request = Some(request);
        self
    }
    
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }
    
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }
    
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.additional.insert(key.into(), value.into());
        self
    }
    
    pub fn correlation_id(&self) -> Option<&str> {
        self.request.as_ref().map(|r| r.correlation_id.as_str())
    }
}

/// Error response format with optional debug information
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetail,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<DebugInfo>,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub category: String,
}

#[derive(Debug, Serialize)]
pub struct DebugInfo {
    pub error_chain: Vec<String>,
    pub context: ErrorContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
}

/// Comprehensive error types for the API layer
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Validation error: {message}")]
    Validation { message: String },
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    #[error("Parse error: {message}")]
    Parse { message: String },
    
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },
    
    #[error("Internal server error: {message}")]
    Internal { message: String },
    
    #[error("Authentication required")]
    Unauthorized,
    
    #[error("Insufficient permissions")]
    Forbidden,
}

impl ApiError {
    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation { message: message.into() }
    }
    
    /// Create a configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration { message: message.into() }
    }
    
    /// Create a parse error
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse { message: message.into() }
    }
    
    /// Create a not found error
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound { resource: resource.into() }
    }
    
    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }

    /// Get the HTTP status code for this error (primarily for testing)
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Validation { .. } => StatusCode::BAD_REQUEST,
            ApiError::Configuration { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Parse { .. } => StatusCode::BAD_REQUEST,
            ApiError::NotFound { .. } => StatusCode::NOT_FOUND,
            ApiError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
        }
    }

    /// Log the error with structured information
    pub fn log_error(&self, request_id: Option<&str>) {
        let error_type = match self {
            ApiError::Validation { .. } => "validation",
            ApiError::Configuration { .. } => "configuration", 
            ApiError::Parse { .. } => "parse",
            ApiError::NotFound { .. } => "not_found",
            ApiError::Internal { .. } => "internal",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden => "forbidden",
        };

        tracing::error!(
            error = %self,
            error_type = error_type,
            request_id = request_id,
            "API error occurred"
        );
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::Validation { message } => (StatusCode::BAD_REQUEST, message),
            ApiError::Configuration { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
            ApiError::Parse { message } => (StatusCode::BAD_REQUEST, message),
            ApiError::NotFound { resource } => (StatusCode::NOT_FOUND, format!("{} not found", resource)),
            ApiError::Internal { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Authentication required".to_string()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "Insufficient permissions".to_string()),
        };

        let body = Json(json!({
            "success": false,
            "data": null,
            "message": error_message
        }));

        (status, body).into_response()
    }
}

// Convert from common error types
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::internal(err.to_string())
    }
}

impl From<crate::config::validation::ValidationError> for ApiError {
    fn from(err: crate::config::validation::ValidationError) -> Self {
        ApiError::validation(err.to_string())
    }
}

impl From<crate::storage::LoadBalancingPolicyParseError> for ApiError {
    fn from(err: crate::storage::LoadBalancingPolicyParseError) -> Self {
        ApiError::parse(err.to_string())
    }
}

impl From<ValidationErrors> for ApiError {
    fn from(errors: ValidationErrors) -> Self {
        let error_messages: Vec<String> = errors
            .field_errors()
            .iter()
            .flat_map(|(field, field_errors)| {
                let field = field.to_string();
                field_errors.iter().map(move |error| {
                    let message = match error.code.as_ref() {
                        "length" => format!("{} length is invalid", field),
                        "range" => format!("{} value is out of range", field),
                        "invalid_route_name" => format!("{} contains invalid characters (only alphanumeric, underscore, hyphen allowed)", field),
                        "invalid_cluster_name" => format!("{} contains invalid characters (only alphanumeric, underscore, period, hyphen allowed)", field),
                        "invalid_host" => format!("{} contains invalid characters", field),
                        "invalid_path_format" => format!("{} must start with / and contain only safe URL characters", field),
                        "path_traversal_detected" => format!("{} contains path traversal attempt (.. or //)", field),
                        "invalid_http_method" => format!("{} contains invalid HTTP method", field),
                        "invalid_lb_policy" => format!("{} contains invalid load balancing policy", field),
                        "empty_http_methods" => format!("{} cannot be empty", field),
                        "too_many_http_methods" => format!("{} contains too many methods (max 10)", field),
                        _ => format!("{} validation failed: {}", field, error.code),
                    };
                    message
                })
            })
            .collect();

        ApiError::validation(error_messages.join(", "))
    }
}

/// Helper trait for adding context to errors
pub trait ErrorContextExt<T> {
    /// Add context to any error result
    fn with_context(self, context: &str) -> Result<T, ApiError>;
}

impl<T, E: Into<ApiError>> ErrorContextExt<T> for Result<T, E> {
    fn with_context(self, context: &str) -> Result<T, ApiError> {
        self.map_err(|e| {
            let mut error = e.into();
            // Enhance error message with context
            match &mut error {
                ApiError::Validation { message } => {
                    *message = format!("{} (context: {})", message, context);
                }
                ApiError::Configuration { message } => {
                    *message = format!("{} (context: {})", message, context);
                }
                ApiError::Parse { message } => {
                    *message = format!("{} (context: {})", message, context);
                }
                ApiError::Internal { message } => {
                    *message = format!("{} (context: {})", message, context);
                }
                _ => {} // No additional context for other error types
            }
            error
        })
    }
}

/// Helper trait for safer parsing operations
pub trait SafeParse<T> {
    /// Parse with detailed error context
    fn safe_parse(&self, context: &str) -> Result<T, ApiError>;
}

impl SafeParse<crate::storage::LoadBalancingPolicy> for str {
    fn safe_parse(&self, context: &str) -> Result<crate::storage::LoadBalancingPolicy, ApiError> {
        use std::str::FromStr;
        
        crate::storage::LoadBalancingPolicy::from_str(self)
            .map_err(|_| ApiError::parse(format!("Invalid load balancing policy '{}' in {}", self, context)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    
    #[test]
    fn test_api_error_status_codes() {
        let validation_err = ApiError::validation("test validation error");
        let response = validation_err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        
        let not_found_err = ApiError::not_found("test resource");
        let response = not_found_err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        
        let internal_err = ApiError::internal("test internal error");
        let response = internal_err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    #[test]
    fn test_safe_parse_trait() {
        // Test valid load balancing policy
        let result = "ROUND_ROBIN".safe_parse("test context");
        assert!(result.is_ok());
        
        // Test with Custom variant (which should work)
        let result = "CUSTOM_POLICY".safe_parse("test context");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_context_trait() {
        use super::ErrorContext;
        
        // Test adding context to validation error
        let result: Result<(), ApiError> = Err(ApiError::validation("original message"));
        let result_with_context = result.with_context("during cluster creation");
        
        if let Err(ApiError::Validation { message }) = result_with_context {
            assert!(message.contains("original message"));
            assert!(message.contains("context: during cluster creation"));
        } else {
            panic!("Expected validation error with context");
        }
    }
}