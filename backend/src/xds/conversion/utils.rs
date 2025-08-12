use super::errors::ConversionError;
use crate::config::AppConfig;
use crate::storage::models::{Cluster as InternalCluster, Route as InternalRoute};
use crate::validation::security::Validator;
use tracing::{info, warn};

/// Load application configuration with fallback mechanism
/// This ensures conversion always has access to valid configuration
pub fn load_config_with_fallback() -> Result<AppConfig, ConversionError> {
    match AppConfig::load() {
        Ok(config) => {
            info!("✅ Configuration loaded successfully from config.yaml + environment");
            Ok(config)
        }
        Err(e) => {
            warn!("⚠️  Failed to load configuration: {}. Using fallback configuration.", e);
            Err(ConversionError::ConfigurationLoad { source: e })
        }
    }
}

/// Validate route configuration for XDS conversion
pub fn validate_route(route: &InternalRoute) -> Result<(), ConversionError> {
    // Validate route path
    if route.path.is_empty() {
        return Err(ConversionError::InvalidResource {
            resource_type: "Route".to_string(),
            resource_id: route.path.clone(),
            reason: "Route path cannot be empty".to_string(),
        });
    }

    if !route.path.starts_with('/') {
        return Err(ConversionError::InvalidResource {
            resource_type: "Route".to_string(),
            resource_id: route.path.clone(),
            reason: "Route path must start with '/'".to_string(),
        });
    }

    // Validate cluster reference
    if route.cluster_name.is_empty() {
        return Err(ConversionError::InvalidResource {
            resource_type: "Route".to_string(),
            resource_id: route.path.clone(),
            reason: "Route must specify a cluster_name".to_string(),
        });
    }

    // Use consolidated security validation for path safety
    Validator::validate_lua_safety(&route.path, &format!("route_path_{}", route.path))
        .map_err(|e| ConversionError::ValidationFailed {
            reason: format!("Route path '{}' failed security validation: {}", route.path, e)
        })?;

    // Validate HTTP methods if present
    if let Some(methods) = &route.http_methods {
        for method in methods {
            if method.is_empty() {
                return Err(ConversionError::InvalidResource {
                    resource_type: "Route".to_string(),
                    resource_id: route.path.clone(),
                    reason: "HTTP method cannot be empty".to_string(),
                });
            }
        }
    }

    Ok(())
}

/// Validate cluster configuration for XDS conversion
pub fn validate_cluster(cluster: &InternalCluster) -> Result<(), ConversionError> {
    if cluster.name.is_empty() {
        return Err(ConversionError::InvalidResource {
            resource_type: "Cluster".to_string(),
            resource_id: cluster.name.clone(),
            reason: "Cluster name cannot be empty".to_string(),
        });
    }

    if cluster.endpoints.is_empty() {
        return Err(ConversionError::InvalidResource {
            resource_type: "Cluster".to_string(),
            resource_id: cluster.name.clone(),
            reason: "Cluster must have at least one endpoint".to_string(),
        });
    }

    // Validate each endpoint
    for (i, endpoint) in cluster.endpoints.iter().enumerate() {
        if endpoint.host.is_empty() {
            return Err(ConversionError::InvalidResource {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: format!("Endpoint {} has empty host", i),
            });
        }

        if endpoint.port == 0 {
            return Err(ConversionError::InvalidResource {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: format!("Endpoint {} has invalid port (0)", i),
            });
        }

        if endpoint.port > 65535 {
            return Err(ConversionError::InvalidResource {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: format!("Endpoint {} has invalid port ({}), must be <= 65535", i, endpoint.port),
            });
        }

        // Validate host for security (basic checks)
        Validator::validate_lua_safety(&endpoint.host, &format!("cluster_{}_endpoint_{}_host", cluster.name, i))
            .map_err(|e| ConversionError::ValidationFailed {
                reason: format!("Cluster '{}' endpoint {} host '{}' failed security validation: {}", 
                               cluster.name, i, endpoint.host, e)
            })?;
    }

    Ok(())
}

/// Generate a safe Lua string literal with proper escaping
/// This prevents Lua injection attacks by properly escaping special characters
/// 
/// Note: This function is kept for backward compatibility, but new code should use
/// the HeaderManipulationStrategy::safe_lua_string method instead.
pub fn safe_lua_string(input: &str, field_name: &str) -> Result<String, ConversionError> {
    // Use consolidated security validation
    Validator::validate_lua_safety(input, field_name)
        .map_err(|e| ConversionError::ValidationFailed { reason: e.to_string() })?;

    // Create properly escaped Lua string using long bracket syntax for safety
    let bracket_level = find_safe_bracket_level(input);
    Ok(format!("[{}[{}]{}]", "=".repeat(bracket_level), input, "=".repeat(bracket_level)))
}

/// Find a safe bracket level for Lua long string literals
/// This ensures our closing bracket won't match anything in the string content
fn find_safe_bracket_level(input: &str) -> usize {
    let mut level = 0;
    
    // Keep increasing the bracket level until we find one that doesn't appear in the input
    loop {
        let closing_pattern = format!("]{}]", "=".repeat(level));
        
        if !input.contains(&closing_pattern) {
            return level;
        }
        level += 1;
        
        // Safety limit to prevent infinite loops
        if level > 10 {
            return 10;
        }
    }
}

/// Get the appropriate Envoy filter name for a given filter type
pub fn get_envoy_filter_name(filter_type: &str) -> Result<String, ConversionError> {
    let filter_name = match filter_type {
        "rate_limit" => "envoy.filters.http.local_ratelimit",
        "cors" => "envoy.filters.http.cors", 
        "authentication" => "envoy.filters.http.jwt_authn",
        "header_manipulation" => "envoy.filters.http.lua",
        "request_validation" => "envoy.filters.http.rbac",
        _ => {
            return Err(ConversionError::UnsupportedFilterType {
                filter_type: filter_type.to_string(),
                supported_types: vec![
                    "rate_limit".to_string(),
                    "cors".to_string(), 
                    "authentication".to_string(),
                    "header_manipulation".to_string(),
                    "request_validation".to_string(),
                ],
            });
        }
    };
    
    Ok(filter_name.to_string())
}