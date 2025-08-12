use thiserror::Error;

/// Errors that can occur during XDS resource conversion
#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Configuration load failed: {source}")]
    ConfigurationLoad { 
        source: anyhow::Error 
    },
    
    #[error("Protobuf encoding failed for {resource_type}: {source}")]
    ProtobufEncoding { 
        resource_type: String, 
        source: prost::EncodeError 
    },
    
    #[error("Invalid resource configuration: {resource_type} '{resource_id}' - {reason}")]
    InvalidResource { 
        resource_type: String, 
        resource_id: String, 
        reason: String 
    },
    
    #[error("Resource dependency missing: {resource_type} '{resource_id}' requires {dependency}")]
    MissingDependency { 
        resource_type: String, 
        resource_id: String, 
        dependency: String 
    },
    
    #[error("Storage operation failed: {source}")]
    StorageError {
        #[from]
        source: crate::storage::StorageError
    },
    
    #[error("Resource validation failed: {reason}")]
    ValidationFailed {
        reason: String,
    },

    #[error("Unsupported filter type '{filter_type}'. Supported types: {supported_types:?}")]
    UnsupportedFilterType {
        filter_type: String,
        supported_types: Vec<String>,
    },
}