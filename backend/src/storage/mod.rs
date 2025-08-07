use thiserror::Error;

pub mod models;
pub mod store;

pub use models::*;
pub use store::*;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Resource not found: {resource_type} '{resource_id}'")]
    ResourceNotFound { 
        resource_type: String, 
        resource_id: String 
    },
    
    #[error("Resource already exists: {resource_type} '{resource_id}'")]
    ResourceConflict { 
        resource_type: String, 
        resource_id: String 
    },
    
    #[error("Storage capacity exceeded: {current}/{limit}")]
    CapacityExceeded { 
        current: usize, 
        limit: usize 
    },
    
    #[error("Invalid resource state: {reason}")]
    InvalidState { 
        reason: String 
    },
    
    #[error("Concurrent modification detected for {resource_type} '{resource_id}'")]
    ConcurrentModification { 
        resource_type: String, 
        resource_id: String 
    },
    
    #[error("Storage backend error: {source}")]
    Backend { 
        source: Box<dyn std::error::Error + Send + Sync> 
    },
    
    #[error("Resource validation failed: {resource_type} '{resource_id}' - {reason}")]
    ValidationFailed {
        resource_type: String,
        resource_id: String,
        reason: String,
    },
    
    #[error("Dependency validation failed: {resource_type} '{resource_id}' requires {dependency}")]
    DependencyMissing {
        resource_type: String,
        resource_id: String,
        dependency: String,
    },
}

impl From<StorageError> for crate::api::errors::ApiError {
    fn from(err: StorageError) -> Self {
        use crate::api::errors::ApiError;
        
        match err {
            StorageError::ResourceNotFound { resource_type, resource_id } => {
                ApiError::not_found(format!("{} '{}' not found", resource_type, resource_id))
            },
            StorageError::ResourceConflict { resource_type, resource_id } => {
                ApiError::validation(format!("{} '{}' already exists", resource_type, resource_id))
            },
            StorageError::CapacityExceeded { current, limit } => {
                ApiError::internal(format!("Storage capacity exceeded: {}/{}", current, limit))
            },
            StorageError::InvalidState { reason } => {
                ApiError::validation(format!("Invalid resource state: {}", reason))
            },
            StorageError::ConcurrentModification { resource_type, resource_id } => {
                ApiError::validation(format!("Concurrent modification detected for {} '{}'", resource_type, resource_id))
            },
            StorageError::Backend { source: _ } => {
                ApiError::internal("Storage backend error".to_string())
            },
            StorageError::ValidationFailed { resource_type, resource_id, reason } => {
                ApiError::validation(format!("{} '{}' validation failed: {}", resource_type, resource_id, reason))
            },
            StorageError::DependencyMissing { resource_type, resource_id, dependency } => {
                ApiError::validation(format!("{} '{}' requires missing dependency: {}", resource_type, resource_id, dependency))
            },
        }
    }
}
