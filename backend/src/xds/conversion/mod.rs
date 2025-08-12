//! XDS resource conversion module
//! 
//! This module handles the conversion of internal data structures to Envoy protobuf format.
//! It's organized into focused submodules for better maintainability:
//! 
//! - `clusters`: Cluster conversion logic
//! - `routes`: Route conversion logic  
//! - `listeners`: Listener and HTTP filter conversion logic (integrates FilterStrategyRegistry)
//! - `utils`: Shared utilities and validation functions
//! - `errors`: Error types for conversion operations
//!
//! ## Key Features
//! - **Strategy Pattern Integration**: HTTP filters use the FilterStrategyRegistry for modular conversion
//! - **Security Validation**: All conversions use consolidated security validation
//! - **Configuration-Driven**: Strategies adapt based on AppConfig settings
//! - **Comprehensive Error Handling**: Detailed error information for troubleshooting

pub mod clusters;
pub mod routes;
pub mod listeners;
pub mod utils;
pub mod errors;

// Re-export for backward compatibility and easy access
pub use errors::ConversionError;
pub use clusters::clusters_to_proto;
pub use routes::routes_to_proto;
pub use listeners::{listeners_to_proto, convert_http_filters};

use crate::storage::ConfigStore;
use prost_types::Any;
use tracing::info;

/// Main conversion entry point for XDS resources
/// 
/// This function routes different resource types to their appropriate conversion modules.
/// It maintains the same interface as the original monolithic conversion.rs for compatibility.
pub fn get_resources_by_type(type_url: &str, store: &ConfigStore) -> Result<Vec<Any>, ConversionError> {
    info!("🔄 Converting resources for type: {}", type_url);
    
    match type_url {
        "type.googleapis.com/envoy.config.cluster.v3.Cluster" => {
            let cluster_list = store.list_clusters();
            clusters_to_proto(cluster_list.iter().map(|c| (**c).clone()).collect())
        }

        "type.googleapis.com/envoy.config.route.v3.RouteConfiguration" => {
            let route_list = store.list_routes();
            routes_to_proto(route_list.iter().map(|r| (**r).clone()).collect())
        }

        "type.googleapis.com/envoy.config.listener.v3.Listener" => {
            listeners_to_proto(store)
        }

        // For other types (endpoints, etc.) return empty for now
        _ => {
            info!("Unsupported resource type: {}", type_url);
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_supported_resource_types() {
        let store = ConfigStore::new();
        
        // Test supported types
        let cluster_result = get_resources_by_type("type.googleapis.com/envoy.config.cluster.v3.Cluster", &store);
        assert!(cluster_result.is_ok());
        
        let route_result = get_resources_by_type("type.googleapis.com/envoy.config.route.v3.RouteConfiguration", &store);
        assert!(route_result.is_ok());
        
        let listener_result = get_resources_by_type("type.googleapis.com/envoy.config.listener.v3.Listener", &store);
        assert!(listener_result.is_ok());
        
        // Test unsupported type
        let unsupported_result = get_resources_by_type("type.googleapis.com/unsupported.Type", &store);
        assert!(unsupported_result.is_ok()); // Returns empty vec, doesn't error
    }
}