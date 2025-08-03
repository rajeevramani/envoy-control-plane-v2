use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use thiserror::Error;

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

/// Helper trait for adding context to errors
pub trait ErrorContext<T> {
    /// Add context to any error result
    fn with_context(self, context: &str) -> Result<T, ApiError>;
}

impl<T, E: Into<ApiError>> ErrorContext<T> for Result<T, E> {
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