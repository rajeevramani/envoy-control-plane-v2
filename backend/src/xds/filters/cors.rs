use crate::storage::HttpFilter as InternalHttpFilter;
use crate::xds::conversion::ConversionError;
use crate::xds::filters::FilterStrategy;
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType;
use envoy_types::pb::envoy::extensions::filters::http::cors::v3::Cors;
use envoy_types::pb::google::protobuf::Any;
use tracing::info;

/// Strategy for converting CORS filters to Envoy CORS
pub struct CorsStrategy;

impl FilterStrategy for CorsStrategy {
    fn filter_type(&self) -> &'static str {
        "cors"
    }

    fn validate(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        // Validate allowed_origins if present
        if let Some(origins) = filter.config.get("allowed_origins") {
            let origins_array = origins.as_array()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("CORS allowed_origins must be an array for filter '{}'", filter.name)
                })?;

            for (i, origin) in origins_array.iter().enumerate() {
                let origin_str = origin.as_str()
                    .ok_or_else(|| ConversionError::ValidationFailed {
                        reason: format!("CORS allowed_origins[{}] must be a string for filter '{}'", i, filter.name)
                    })?;

                crate::validation::security::Validator::validate_length(origin_str, "cors_origin", Some(1), Some(253))
                    .map_err(ConversionError::from)?;
                crate::validation::security::Validator::validate_lua_safety(origin_str, "cors_origin")
                    .map_err(ConversionError::from)?;
            }
        }

        // Validate allowed_methods if present
        if let Some(methods) = filter.config.get("allowed_methods") {
            let methods_array = methods.as_array()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("CORS allowed_methods must be an array for filter '{}'", filter.name)
                })?;

            for (i, method) in methods_array.iter().enumerate() {
                let method_str = method.as_str()
                    .ok_or_else(|| ConversionError::ValidationFailed {
                        reason: format!("CORS allowed_methods[{}] must be a string for filter '{}'", i, filter.name)
                    })?;

                match method_str {
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => {},
                    _ => return Err(ConversionError::ValidationFailed {
                        reason: format!("Invalid HTTP method '{}' in CORS allowed_methods for filter '{}'", 
                            method_str, filter.name)
                    })
                }
            }
        }

        Ok(())
    }

    fn convert(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError> {
        info!("Converting CORS filter '{}' to Envoy CORS", filter.name);

        // Extract CORS config from our JSON
        let allowed_origins = filter.config.get("allowed_origins")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|| vec!["*".to_string()]);

        let allowed_methods = filter.config.get("allowed_methods")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|| vec!["GET".to_string(), "POST".to_string()]);

        let allowed_headers = filter.config.get("allowed_headers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|| vec!["Content-Type".to_string(), "Authorization".to_string()]);

        let allow_credentials = filter.config.get("allow_credentials")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Create basic CORS configuration (following existing simplified pattern)
        // TODO: In future versions, we can expand this to use the extracted config values
        let cors_config = Cors::default();

        // Serialize to Any proto
        let any_config = Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors".to_string(),
            value: {
                let mut buf = Vec::new();
                prost::Message::encode(&cors_config, &mut buf)
                    .map_err(|e| ConversionError::ProtobufEncoding {
                        resource_type: "Cors".to_string(),
                        source: e,
                    })?;
                buf
            },
        };

        Ok(ConfigType::TypedConfig(any_config))
    }

    fn description(&self) -> &'static str {
        "Cross-Origin Resource Sharing (CORS) filter for handling cross-origin requests"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cors_validation() {
        let strategy = CorsStrategy;
        
        // Valid configuration
        let valid_filter = InternalHttpFilter {
            name: "test-cors".to_string(),
            filter_type: "cors".to_string(),
            enabled: true,
            config: json!({
                "allowed_origins": ["https://example.com"],
                "allowed_methods": ["GET", "POST"]
            }),
        };
        
        assert!(strategy.validate(&valid_filter).is_ok());
        
        // Invalid configuration - invalid HTTP method
        let invalid_filter = InternalHttpFilter {
            name: "test-invalid".to_string(),
            filter_type: "cors".to_string(),
            enabled: true,
            config: json!({
                "allowed_methods": ["INVALID"]
            }),
        };
        
        assert!(strategy.validate(&invalid_filter).is_err());
    }

    #[test]
    fn test_cors_conversion() {
        let strategy = CorsStrategy;
        
        let filter = InternalHttpFilter {
            name: "test-cors".to_string(),
            filter_type: "cors".to_string(),
            enabled: true,
            config: json!({
                "allowed_origins": ["https://example.com"],
                "allowed_methods": ["GET", "POST"],
                "allow_credentials": true
            }),
        };
        
        let result = strategy.convert(&filter);
        assert!(result.is_ok());
        
        if let Ok(ConfigType::TypedConfig(any)) = result {
            assert_eq!(any.type_url, "type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors");
        } else {
            panic!("Expected TypedConfig result");
        }
    }

    #[test]
    fn test_cors_defaults() {
        let strategy = CorsStrategy;
        
        // Empty config should use defaults
        let filter = InternalHttpFilter {
            name: "test-cors-defaults".to_string(),
            filter_type: "cors".to_string(),
            enabled: true,
            config: json!({}),
        };
        
        let result = strategy.convert(&filter);
        assert!(result.is_ok(), "Should handle empty config with defaults");
    }
}