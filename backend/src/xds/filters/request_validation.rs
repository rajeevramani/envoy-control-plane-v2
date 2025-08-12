use crate::config::AppConfig;
use crate::storage::HttpFilter as InternalHttpFilter;
use crate::xds::conversion::ConversionError;
use crate::xds::filters::FilterStrategy;
use crate::validation::security::Validator;
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType;
use envoy_types::pb::envoy::extensions::filters::http::rbac::v3::Rbac;
use envoy_types::pb::envoy::r#type::matcher::v3::RegexMatcher;
use envoy_types::pb::google::protobuf::Any;
use prost::Message;
use tracing::info;
use std::collections::HashMap;

/// Strategy for converting request validation filters to Envoy RBAC
/// 
/// Request validation is implemented using Envoy's RBAC (Role-Based Access Control) filter
/// to validate requests based on headers, paths, methods, and other attributes.
pub struct RequestValidationStrategy {
    app_config: AppConfig,
}

impl RequestValidationStrategy {
    pub fn new(app_config: AppConfig) -> Self {
        Self { app_config }
    }
}

impl FilterStrategy for RequestValidationStrategy {
    fn filter_type(&self) -> &'static str {
        "request_validation"
    }

    fn validate(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        // Validate allowed_methods if present
        if let Some(methods) = filter.config.get("allowed_methods") {
            let methods_array = methods.as_array()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("allowed_methods must be an array for filter '{}'", filter.name)
                })?;

            for method in methods_array {
                let method_str = method.as_str()
                    .ok_or_else(|| ConversionError::ValidationFailed {
                        reason: format!("Each allowed method must be a string for filter '{}'", filter.name)
                    })?;

                // Validate HTTP method format
                if !method_str.chars().all(|c| c.is_ascii_uppercase()) {
                    return Err(ConversionError::ValidationFailed {
                        reason: format!("HTTP method '{}' must be uppercase for filter '{}'", method_str, filter.name)
                    });
                }

                // Check against configured supported methods from config.yaml
                if !self.app_config.control_plane.http_methods.supported_methods.contains(&method_str.to_string()) {
                    return Err(ConversionError::ValidationFailed {
                        reason: format!("HTTP method '{}' is not supported for filter '{}'. Supported methods from config: {:?}", 
                            method_str, filter.name, self.app_config.control_plane.http_methods.supported_methods)
                    });
                }
            }
        }

        // Validate required_headers if present
        if let Some(headers) = filter.config.get("required_headers") {
            let headers_array = headers.as_array()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("required_headers must be an array for filter '{}'", filter.name)
                })?;

            for header in headers_array {
                let header_str = header.as_str()
                    .ok_or_else(|| ConversionError::ValidationFailed {
                        reason: format!("Each required header must be a string for filter '{}'", filter.name)
                    })?;

                // Validate header name format
                Validator::validate_http_header_name(header_str)
                    .map_err(|e| ConversionError::ValidationFailed {
                        reason: format!("Invalid required header name in filter '{}': {}", filter.name, e)
                    })?;
            }
        }

        // Validate allowed_paths if present
        if let Some(paths) = filter.config.get("allowed_paths") {
            let paths_array = paths.as_array()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("allowed_paths must be an array for filter '{}'", filter.name)
                })?;

            for path in paths_array {
                let path_str = path.as_str()
                    .ok_or_else(|| ConversionError::ValidationFailed {
                        reason: format!("Each allowed path must be a string for filter '{}'", filter.name)
                    })?;

                // Validate path format and security
                if path_str.is_empty() {
                    return Err(ConversionError::ValidationFailed {
                        reason: format!("Allowed path cannot be empty for filter '{}'", filter.name)
                    });
                }

                if !path_str.starts_with('/') {
                    return Err(ConversionError::ValidationFailed {
                        reason: format!("Allowed path '{}' must start with '/' for filter '{}'", path_str, filter.name)
                    });
                }

                // Check for path traversal attempts
                if path_str.contains("..") {
                    return Err(ConversionError::ValidationFailed {
                        reason: format!("Path traversal detected in allowed path '{}' for filter '{}'", path_str, filter.name)
                    });
                }

                // Validate for Lua safety (paths can be used in regex patterns)
                Validator::validate_lua_safety(path_str, &format!("allowed_path_for_filter_{}", filter.name))
                    .map_err(|e| ConversionError::ValidationFailed {
                        reason: format!("Security issue in allowed path '{}' for filter '{}': {}", path_str, filter.name, e)
                    })?;
            }
        }

        Ok(())
    }

    fn convert(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError> {
        info!("Converting request_validation filter '{}' to Envoy RBAC", filter.name);

        let allowed_methods = filter.config.get("allowed_methods")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|| vec!["GET".to_string(), "POST".to_string()]);

        let required_headers = filter.config.get("required_headers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let allowed_paths = filter.config.get("allowed_paths")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        // Create RBAC policy for request validation
        let mut policies = HashMap::new();
        let mut and_rules = vec![];
        
        // Add method validation
        if !allowed_methods.is_empty() {
            and_rules.push(envoy_types::pb::envoy::config::rbac::v3::Permission {
                rule: Some(envoy_types::pb::envoy::config::rbac::v3::permission::Rule::Header(
                    envoy_types::pb::envoy::config::route::v3::HeaderMatcher {
                        name: ":method".to_string(),
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::StringMatch(
                                envoy_types::pb::envoy::r#type::matcher::v3::StringMatcher {
                                    match_pattern: Some(
                                        envoy_types::pb::envoy::r#type::matcher::v3::string_matcher::MatchPattern::SafeRegex(
                                            RegexMatcher {
                                                regex: format!("^({})$", allowed_methods.join("|")),
                                                ..Default::default()
                                            }
                                        )
                                    ),
                                    ..Default::default()
                                }
                            )
                        ),
                        ..Default::default()
                    }
                )),
            });
        }
        
        // Add required header validation
        for header in required_headers {
            and_rules.push(envoy_types::pb::envoy::config::rbac::v3::Permission {
                rule: Some(envoy_types::pb::envoy::config::rbac::v3::permission::Rule::Header(
                    envoy_types::pb::envoy::config::route::v3::HeaderMatcher {
                        name: header,
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::PresentMatch(true)
                        ),
                        ..Default::default()
                    }
                )),
            });
        }

        // Add path validation if specified
        if !allowed_paths.is_empty() {
            and_rules.push(envoy_types::pb::envoy::config::rbac::v3::Permission {
                rule: Some(envoy_types::pb::envoy::config::rbac::v3::permission::Rule::Header(
                    envoy_types::pb::envoy::config::route::v3::HeaderMatcher {
                        name: ":path".to_string(),
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::StringMatch(
                                envoy_types::pb::envoy::r#type::matcher::v3::StringMatcher {
                                    match_pattern: Some(
                                        envoy_types::pb::envoy::r#type::matcher::v3::string_matcher::MatchPattern::SafeRegex(
                                            RegexMatcher {
                                                regex: format!("^({})$", allowed_paths.join("|")),
                                                ..Default::default()
                                            }
                                        )
                                    ),
                                    ..Default::default()
                                }
                            )
                        ),
                        ..Default::default()
                    }
                )),
            });
        }

        policies.insert("allow_valid_requests".to_string(), 
            envoy_types::pb::envoy::config::rbac::v3::Policy {
                permissions: vec![envoy_types::pb::envoy::config::rbac::v3::Permission {
                    rule: Some(envoy_types::pb::envoy::config::rbac::v3::permission::Rule::AndRules(
                        envoy_types::pb::envoy::config::rbac::v3::permission::Set {
                            rules: and_rules,
                        }
                    )),
                }],
                principals: vec![envoy_types::pb::envoy::config::rbac::v3::Principal {
                    identifier: Some(envoy_types::pb::envoy::config::rbac::v3::principal::Identifier::Any(true)),
                }],
                ..Default::default()
            }
        );

        let rbac_config = Rbac {
            rules: Some(envoy_types::pb::envoy::config::rbac::v3::Rbac {
                action: envoy_types::pb::envoy::config::rbac::v3::rbac::Action::Allow as i32,
                policies,
                ..Default::default()
            }),
            ..Default::default()
        };

        // Serialize to Any proto
        let any_config = Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC".to_string(),
            value: {
                let mut buf = Vec::new();
                rbac_config.encode(&mut buf)
                    .map_err(|e| ConversionError::ProtobufEncoding {
                        resource_type: "RBAC".to_string(),
                        source: e,
                    })?;
                buf
            },
        };

        Ok(ConfigType::TypedConfig(any_config))
    }

    fn description(&self) -> &'static str {
        "Request validation filter using Envoy's RBAC for secure validation of methods, headers, and paths"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::config::*;
    use std::path::PathBuf;

    fn create_test_config() -> AppConfig {
        AppConfig {
            control_plane: ControlPlaneConfig {
                server: ServerConfig {
                    rest_port: 8080,
                    xds_port: 18000,
                    host: "0.0.0.0".to_string(),
                },
                tls: TlsConfig {
                    cert_path: "./certs/server.crt".to_string(),
                    key_path: "./certs/server.key".to_string(),
                    enabled: true,
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                },
                load_balancing: LoadBalancingConfig {
                    envoy_version: "1.24".to_string(),
                    available_policies: vec!["ROUND_ROBIN".to_string()],
                    default_policy: "ROUND_ROBIN".to_string(),
                },
                http_methods: HttpMethodsConfig {
                    supported_methods: vec![
                        "GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string()
                    ],
                },
                authentication: AuthenticationConfig {
                    enabled: false,
                    jwt_secret: "test-secret-1234567890abcdefghijklmnopqrstuvwxyz".to_string(),
                    jwt_expiry_hours: 24,
                    jwt_issuer: "envoy-control-plane-test".to_string(),
                    password_hash_cost: 8,
                },
                storage: StorageConfig::default(),
                http_filters: HttpFiltersFeatureConfig::default(),
            },
            envoy_generation: EnvoyGenerationConfig {
                config_dir: PathBuf::from("./configs"),
                admin: AdminConfig {
                    host: "127.0.0.1".to_string(),
                    port: 9901,
                },
                listener: ListenerConfig {
                    binding_address: "0.0.0.0".to_string(),
                    default_port: 10000,
                },
                cluster: ClusterConfig {
                    connect_timeout_seconds: 5,
                    discovery_type: "STRICT_DNS".to_string(),
                    dns_lookup_family: "V4_ONLY".to_string(),
                    default_protocol: "TCP".to_string(),
                },
                naming: NamingConfig {
                    listener_name: "listener_0".to_string(),
                    virtual_host_name: "local_service".to_string(),
                    route_config_name: "local_route".to_string(),
                    default_domains: vec!["*".to_string()],
                },
                bootstrap: BootstrapConfig {
                    node_id: "envoy-test-node".to_string(),
                    node_cluster: "envoy-test-cluster".to_string(),
                    control_plane_host: "control-plane".to_string(),
                    main_listener_name: "main_listener".to_string(),
                    control_plane_cluster_name: "control_plane_cluster".to_string(),
                },
                http_filters: HttpFiltersConfig {
                    stat_prefix: "ingress_http".to_string(),
                    router_filter_name: "envoy.filters.http.router".to_string(),
                    hcm_filter_name: "envoy.filters.network.http_connection_manager".to_string(),
                },
            },
        }
    }

    #[test]
    fn test_request_validation_validation() {
        let app_config = create_test_config();
        let strategy = RequestValidationStrategy::new(app_config);
        
        // Valid configuration
        let valid_filter = InternalHttpFilter {
            name: "test-request-validation".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_methods": ["GET", "POST"],
                "required_headers": ["Authorization", "Content-Type"],
                "allowed_paths": ["/api/v1/users", "/api/v1/orders"]
            }),
        };
        
        assert!(strategy.validate(&valid_filter).is_ok());
        
        // Invalid configuration - lowercase method
        let invalid_filter = InternalHttpFilter {
            name: "test-invalid".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_methods": ["get", "POST"]
            }),
        };
        
        assert!(strategy.validate(&invalid_filter).is_err());
    }

    #[test]
    fn test_unsupported_method_validation() {
        let app_config = create_test_config();
        let strategy = RequestValidationStrategy::new(app_config);
        
        // Invalid method - not in supported_methods from config
        let invalid_filter = InternalHttpFilter {
            name: "test-unsupported-method".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_methods": ["INVALID_METHOD"]
            }),
        };
        
        assert!(strategy.validate(&invalid_filter).is_err());
    }

    #[test]
    fn test_path_validation() {
        let app_config = create_test_config();
        let strategy = RequestValidationStrategy::new(app_config);
        
        // Invalid path - path traversal
        let invalid_filter = InternalHttpFilter {
            name: "test-path-traversal".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_paths": ["/api/../sensitive"]
            }),
        };
        
        assert!(strategy.validate(&invalid_filter).is_err());
        
        // Invalid path - doesn't start with /
        let invalid_filter2 = InternalHttpFilter {
            name: "test-invalid-path".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_paths": ["api/users"]
            }),
        };
        
        assert!(strategy.validate(&invalid_filter2).is_err());
    }

    #[test]
    fn test_request_validation_conversion() {
        let app_config = create_test_config();
        let strategy = RequestValidationStrategy::new(app_config);
        
        let filter = InternalHttpFilter {
            name: "test-request-validation".to_string(),
            filter_type: "request_validation".to_string(),
            enabled: true,
            config: json!({
                "allowed_methods": ["GET", "POST", "PUT"],
                "required_headers": ["Authorization"],
                "allowed_paths": ["/api/v1/.*"]
            }),
        };
        
        let result = strategy.convert(&filter);
        assert!(result.is_ok());
        
        if let Ok(ConfigType::TypedConfig(any)) = result {
            assert_eq!(any.type_url, "type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC");
        } else {
            panic!("Expected TypedConfig result");
        }
    }
}