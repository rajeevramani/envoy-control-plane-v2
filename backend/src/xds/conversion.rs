use crate::storage::models::{
    Cluster as InternalCluster, LoadBalancingPolicy, Route as InternalRoute, HttpFilter as InternalHttpFilter,
};
use prost::Message;
use prost_types::Any;
use envoy_types::pb::google::protobuf::Any as EnvoyAny;
use tracing::{info, warn};
use thiserror::Error;

// Import actual Envoy protobuf types
use envoy_types::pb::envoy::config::cluster::v3::Cluster;
use envoy_types::pb::envoy::config::core::v3::{Address, SocketAddress};
use envoy_types::pb::envoy::config::endpoint::v3::{
    ClusterLoadAssignment, Endpoint, LbEndpoint, LocalityLbEndpoints,
};
use envoy_types::pb::envoy::config::route::v3::{
    HeaderMatcher, Route, RouteAction, RouteConfiguration, RouteMatch, VirtualHost,
};
use envoy_types::pb::envoy::config::listener::v3::{Listener, FilterChain, Filter};
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::{
    HttpConnectionManager, Rds, HttpFilter,
};
use envoy_types::pb::envoy::extensions::filters::http::router::v3::Router;
use envoy_types::pb::envoy::extensions::filters::http::fault::v3::HttpFault;
use envoy_types::pb::envoy::extensions::filters::http::local_ratelimit::v3::LocalRateLimit;
use envoy_types::pb::envoy::extensions::filters::http::cors::v3::Cors;
use envoy_types::pb::envoy::config::core::v3::{HeaderValue, HeaderValueOption};
use envoy_types::pb::envoy::r#type::matcher::v3::{RegexMatcher, StringMatcher};
use base64::prelude::*;

// Include the generated protobuf code for ADS
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

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
}

impl From<ConversionError> for crate::api::errors::ApiError {
    fn from(err: ConversionError) -> Self {
        use crate::api::errors::ApiError;
        
        match err {
            ConversionError::ConfigurationLoad { source: _ } => {
                ApiError::internal("Configuration load failed".to_string())
            },
            ConversionError::ProtobufEncoding { resource_type, source: _ } => {
                ApiError::internal(format!("Protobuf encoding failed for {}", resource_type))
            },
            ConversionError::InvalidResource { resource_type, resource_id, reason } => {
                ApiError::validation(format!("{} '{}': {}", resource_type, resource_id, reason))
            },
            ConversionError::MissingDependency { resource_type, resource_id, dependency } => {
                ApiError::validation(format!("{} '{}' missing dependency: {}", resource_type, resource_id, dependency))
            },
            ConversionError::StorageError { source } => {
                // Convert StorageError to ApiError using existing conversion
                source.into()
            },
            ConversionError::ValidationFailed { reason } => {
                ApiError::validation(reason)
            },
        }
    }
}

pub struct ProtoConverter;

impl ProtoConverter {
    /// Convert our LoadBalancingPolicy enum to Envoy's protobuf LbPolicy
    fn lb_policy_to_envoy_proto(policy: &LoadBalancingPolicy) -> i32 {
        use envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy;

        match policy {
            LoadBalancingPolicy::RoundRobin => LbPolicy::RoundRobin as i32,
            LoadBalancingPolicy::LeastRequest => LbPolicy::LeastRequest as i32,
            LoadBalancingPolicy::Random => LbPolicy::Random as i32,
            LoadBalancingPolicy::RingHash => LbPolicy::RingHash as i32,
            LoadBalancingPolicy::Custom(policy_name) => {
                // For custom policies, we'll need to handle them specially
                // For now, log a warning and fall back to RoundRobin
                warn!("Custom policy '{}' not directly supported in protobuf enum, using RoundRobin", policy_name);
                LbPolicy::RoundRobin as i32
            }
        }
    }
}

impl ProtoConverter {
    /// Load configuration with fallback mechanism for resilience
    fn load_config_with_fallback() -> Result<crate::config::AppConfig, ConversionError> {
        crate::config::AppConfig::load()
            .map_err(|e| {
                // Log the configuration load failure but still return error
                warn!("Configuration load failed: {}", e);
                ConversionError::ConfigurationLoad { source: e }
            })
    }
    
    /// Validate route before conversion
    fn validate_route(route: &InternalRoute) -> Result<(), ConversionError> {
        if route.name.is_empty() {
            return Err(ConversionError::InvalidResource {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route name cannot be empty".to_string(),
            });
        }
        
        if route.path.is_empty() {
            return Err(ConversionError::InvalidResource {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route path cannot be empty".to_string(),
            });
        }
        
        if !route.path.starts_with('/') {
            return Err(ConversionError::InvalidResource {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route path must start with '/'".to_string(),
            });
        }
        
        if route.cluster_name.is_empty() {
            return Err(ConversionError::InvalidResource {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route cluster_name cannot be empty".to_string(),
            });
        }
        
        // Validate HTTP methods if provided
        if let Some(ref methods) = route.http_methods {
            for method in methods {
                if !Self::is_valid_http_method(method) {
                    return Err(ConversionError::InvalidResource {
                        resource_type: "Route".to_string(),
                        resource_id: route.name.clone(),
                        reason: format!("Invalid HTTP method: {}", method),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate HTTP method
    fn is_valid_http_method(method: &str) -> bool {
        matches!(method, "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" | "TRACE" | "CONNECT")
    }

    /// Validate JWT authentication filter configuration
    pub fn validate_jwt_auth_config(filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        // Validate JWT secret
        let jwt_secret = filter.config.get("jwt_secret")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() >= 32)
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!(
                    "JWT secret for filter '{}' must be at least 32 characters long and cannot be empty", 
                    filter.name
                )
            })?;

        // Additional security check: ensure secret doesn't contain obvious weak patterns
        if jwt_secret.to_lowercase().contains("secret") || 
           jwt_secret.to_lowercase().contains("password") ||
           jwt_secret == "a".repeat(jwt_secret.len()) ||
           jwt_secret == "1".repeat(jwt_secret.len()) {
            return Err(ConversionError::ValidationFailed {
                reason: format!(
                    "JWT secret for filter '{}' appears to be weak or contain obvious patterns. Use a cryptographically secure random string", 
                    filter.name
                )
            });
        }

        // Validate JWT issuer
        filter.config.get("jwt_issuer")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() <= 100)
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!(
                    "JWT issuer for filter '{}' must be non-empty and at most 100 characters", 
                    filter.name
                )
            })?;

        Ok(())
    }

    /// Convert internal routes to Envoy RouteConfiguration protobuf
    /// Following the Go control plane pattern from makeRoute()
    pub fn routes_to_proto(routes: Vec<InternalRoute>) -> Result<Vec<Any>, ConversionError> {
        if routes.is_empty() {
            return Ok(vec![]);
        }

        // Load config with fallback mechanism
        let app_config = Self::load_config_with_fallback()?;

        info!(
            "Routes conversion: Creating RouteConfiguration with {} routes",
            routes.len()
        );

        // Create routes following the Go control plane pattern with validation
        let mut proto_routes = Vec::new();
        
        for route in routes {
            // Validate route before conversion
            Self::validate_route(&route)?;
            
            info!("  - Route: {} -> {}", route.path, route.cluster_name);

            // Create header matchers for HTTP methods if specified
            let headers = if let Some(ref methods) = route.http_methods {
                if methods.len() == 1 {
                    // Single method - use string match with exact
                    vec![HeaderMatcher {
                        name: ":method".to_string(),
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::StringMatch(
                                StringMatcher {
                                    match_pattern: Some(
                                        envoy_types::pb::envoy::r#type::matcher::v3::string_matcher::MatchPattern::Exact(methods[0].clone())
                                    ),
                                    ..Default::default()
                                }
                            )
                        ),
                        ..Default::default()
                    }]
                } else {
                    // Multiple methods - use regex match
                    let regex_pattern = format!("^({})$", methods.join("|"));
                    vec![HeaderMatcher {
                        name: ":method".to_string(),
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::SafeRegexMatch(
                                RegexMatcher {
                                    regex: regex_pattern,
                                    ..Default::default()
                                }
                            )
                        ),
                        ..Default::default()
                    }]
                }
            } else {
                // No HTTP methods specified - match all methods
                vec![]
            };

            let proto_route = Route {
                r#match: Some(RouteMatch {
                    path_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_match::PathSpecifier::Prefix(route.path)),
                    headers,
                    ..Default::default()
                }),
                action: Some(envoy_types::pb::envoy::config::route::v3::route::Action::Route(RouteAction {
                    cluster_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_action::ClusterSpecifier::Cluster(route.cluster_name)),
                    prefix_rewrite: route.prefix_rewrite.unwrap_or_default(),
                    ..Default::default()
                })),
                ..Default::default()
            };
            
            proto_routes.push(proto_route);
        }

        // Create virtual host with all routes
        let virtual_host = VirtualHost {
            name: app_config.envoy_generation.naming.virtual_host_name.clone(),
            domains: app_config.envoy_generation.naming.default_domains.clone(),
            routes: proto_routes,
            ..Default::default()
        };

        // Create RouteConfiguration
        let route_config = RouteConfiguration {
            name: app_config.envoy_generation.naming.route_config_name.clone(),
            virtual_hosts: vec![virtual_host],
            ..Default::default()
        };

        // Encode to protobuf bytes with proper error handling
        let mut buf = Vec::new();
        route_config.encode(&mut buf)
            .map_err(|e| ConversionError::ProtobufEncoding {
                resource_type: "RouteConfiguration".to_string(),
                source: e,
            })?;

        info!("Routes conversion: Encoded {} bytes", buf.len());

        Ok(vec![Any {
            type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
            value: buf,
        }])
    }

    /// Validate cluster before conversion
    fn validate_cluster(cluster: &InternalCluster) -> Result<(), ConversionError> {
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
                    reason: format!("Endpoint {} host cannot be empty", i + 1),
                });
            }
            
            if endpoint.port == 0 || endpoint.port > 65535 {
                return Err(ConversionError::InvalidResource {
                    resource_type: "Cluster".to_string(),
                    resource_id: cluster.name.clone(),
                    reason: format!("Endpoint {} port {} is invalid (must be 1-65535)", i + 1, endpoint.port),
                });
            }
        }
        
        Ok(())
    }

    /// Convert internal clusters to Envoy Cluster protobuf
    /// Following the Go control plane pattern from makeCluster() and makeEndpoint()
    pub fn clusters_to_proto(clusters: Vec<InternalCluster>) -> Result<Vec<Any>, ConversionError> {
        if clusters.is_empty() {
            return Ok(vec![]);
        }

        // Load config with fallback mechanism
        let app_config = Self::load_config_with_fallback()?;

        info!(
            "Clusters conversion: Creating {} clusters",
            clusters.len()
        );

        let mut proto_clusters = Vec::new();

        for cluster in clusters {
            // Validate cluster before conversion
            Self::validate_cluster(&cluster)?;
            
            let cluster_name = cluster.name.clone(); // Clone before moving
            info!(
                "  - Cluster: {} ({} endpoints)",
                cluster_name,
                cluster.endpoints.len()
            );

            // Create endpoints following the Go control plane pattern
            let lb_endpoints: Vec<LbEndpoint> = cluster.endpoints.into_iter().map(|endpoint| {
                info!("    - Endpoint: {}:{}", endpoint.host, endpoint.port);

                LbEndpoint {
                    host_identifier: Some(envoy_types::pb::envoy::config::endpoint::v3::lb_endpoint::HostIdentifier::Endpoint(
                        Endpoint {
                            address: Some(Address {
                                address: Some(envoy_types::pb::envoy::config::core::v3::address::Address::SocketAddress(
                                    SocketAddress {
                                        protocol: Self::protocol_to_proto(&app_config.envoy_generation.cluster.default_protocol),
                                        address: endpoint.host,
                                        port_specifier: Some(envoy_types::pb::envoy::config::core::v3::socket_address::PortSpecifier::PortValue(endpoint.port as u32)),
                                        ..Default::default()
                                    }
                                )),
                            }),
                            ..Default::default()
                        }
                    )),
                    ..Default::default()
                }
            }).collect();

            // Create load assignment
            let load_assignment = ClusterLoadAssignment {
                cluster_name: cluster_name.clone(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints,
                    ..Default::default()
                }],
                ..Default::default()
            };

            // Create cluster following the Go control plane pattern
            let proto_cluster = Cluster {
                name: cluster.name,
                cluster_discovery_type: Some(envoy_types::pb::envoy::config::cluster::v3::cluster::ClusterDiscoveryType::Type(
                    envoy_types::pb::envoy::config::cluster::v3::cluster::DiscoveryType::StrictDns as i32
                )),
                lb_policy: Self::lb_policy_to_envoy_proto(
                    cluster.lb_policy.as_ref().unwrap_or(&LoadBalancingPolicy::RoundRobin)
                ),
                load_assignment: Some(load_assignment),
                connect_timeout: Some(envoy_types::pb::google::protobuf::Duration {
                    seconds: app_config.envoy_generation.cluster.connect_timeout_seconds as i64,
                    nanos: 0
                }),
                dns_lookup_family: Self::dns_lookup_family_to_proto(&app_config.envoy_generation.cluster.dns_lookup_family),
                ..Default::default()
            };

            // Encode to protobuf bytes with proper error handling
            let mut buf = Vec::new();
            proto_cluster.encode(&mut buf)
                .map_err(|e| ConversionError::ProtobufEncoding {
                    resource_type: format!("Cluster({})", cluster_name),
                    source: e,
                })?;

            info!(
                "Cluster conversion: Encoded {} bytes for {}",
                buf.len(),
                cluster_name
            );

            proto_clusters.push(Any {
                type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
                value: buf,
            });
        }

        Ok(proto_clusters)
    }

    /// Convert internal HTTP filters to Envoy protobuf HTTP filters
    fn convert_http_filters(
        http_filters: Vec<InternalHttpFilter>,
        default_order: &[String],
    ) -> Result<Vec<HttpFilter>, ConversionError> {
        let mut envoy_filters = Vec::new();

        // Apply filters in the configured order
        for filter_type in default_order {
            // Find filters of this type
            let filters_of_type: Vec<&InternalHttpFilter> = http_filters
                .iter()
                .filter(|f| &f.filter_type == filter_type && f.enabled)
                .collect();

            for filter in filters_of_type {
                match filter.filter_type.as_str() {
                    "rate_limit" => {
                        info!("Converting rate_limit filter '{}' to Envoy LocalRateLimit", filter.name);
                        
                        // Extract rate limiting config from our JSON
                        let requests_per_unit = filter.config.get("requests_per_unit")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(100) as u32;
                        
                        let unit = filter.config.get("unit")
                            .and_then(|v| v.as_str())
                            .unwrap_or("minute");
                            
                        let burst_size = filter.config.get("burst_size")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32);

                        // Convert time unit to seconds
                        let time_window_seconds = match unit {
                            "second" => 1,
                            "minute" => 60,
                            "hour" => 3600,
                            "day" => 86400,
                            _ => 60, // default to minute
                        };

                        // Create Envoy LocalRateLimit configuration
                        let rate_limit_config = LocalRateLimit {
                            stat_prefix: format!("rate_limit_{}", filter.name),
                            token_bucket: Some(envoy_types::pb::envoy::r#type::v3::TokenBucket {
                                max_tokens: burst_size.unwrap_or(requests_per_unit * 2),
                                tokens_per_fill: Some(envoy_types::pb::google::protobuf::UInt32Value { value: requests_per_unit }),
                                fill_interval: Some(envoy_types::pb::google::protobuf::Duration {
                                    seconds: time_window_seconds as i64,
                                    nanos: 0,
                                }),
                            }),
                            filter_enabled: Some(envoy_types::pb::envoy::config::core::v3::RuntimeFractionalPercent {
                                default_value: Some(envoy_types::pb::envoy::r#type::v3::FractionalPercent {
                                    numerator: 100,
                                    denominator: envoy_types::pb::envoy::r#type::v3::fractional_percent::DenominatorType::Hundred as i32,
                                }),
                                runtime_key: "rate_limit_enabled".to_string(),
                                ..Default::default()
                            }),
                            filter_enforced: Some(envoy_types::pb::envoy::config::core::v3::RuntimeFractionalPercent {
                                default_value: Some(envoy_types::pb::envoy::r#type::v3::FractionalPercent {
                                    numerator: 100,
                                    denominator: envoy_types::pb::envoy::r#type::v3::fractional_percent::DenominatorType::Hundred as i32,
                                }),
                                runtime_key: "rate_limit_enforced".to_string(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        };

                        let mut buf = Vec::new();
                        rate_limit_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
                            resource_type: "LocalRateLimit".to_string(),
                            source: e,
                        })?;

                        envoy_filters.push(HttpFilter {
                            name: "envoy.filters.http.local_ratelimit".to_string(),
                            config_type: Some(
                                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                                    EnvoyAny {
                                        type_url: "type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit".to_string(),
                                        value: buf,
                                    }
                                )
                            ),
                            ..Default::default()
                        });
                    }
                    "cors" => {
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
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(",")
                            })
                            .unwrap_or_else(|| "GET,POST,PUT,DELETE,OPTIONS".to_string());

                        let allowed_headers = filter.config.get("allowed_headers")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(",")
                            })
                            .unwrap_or_else(|| "Content-Type,Authorization".to_string());

                        let allow_credentials = filter.config.get("allow_credentials")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        // Create basic Envoy CORS configuration (simplified for MVP)
                        let cors_config = Cors::default();

                        let mut buf = Vec::new();
                        cors_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
                            resource_type: "Cors".to_string(),
                            source: e,
                        })?;

                        envoy_filters.push(HttpFilter {
                            name: "envoy.filters.http.cors".to_string(),
                            config_type: Some(
                                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                                    EnvoyAny {
                                        type_url: "type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors".to_string(),
                                        value: buf,
                                    }
                                )
                            ),
                            ..Default::default()
                        });
                    }
                    "header_manipulation" => {
                        info!("Converting header_manipulation filter '{}' to Envoy Lua script", filter.name);
                        
                        // For header manipulation, we'll use a Lua filter since Envoy doesn't have 
                        // a built-in generic header manipulation filter. We'll create Lua code 
                        // that implements the header operations based on our config.
                        
                        let mut lua_script = String::from("function envoy_on_request(request_handle)\n");
                        
                        // Add request headers
                        if let Some(headers_to_add) = filter.config.get("request_headers_to_add").and_then(|v| v.as_array()) {
                            for header in headers_to_add {
                                if let Some(header_obj) = header.get("header") {
                                    if let (Some(key), Some(value)) = (
                                        header_obj.get("key").and_then(|k| k.as_str()),
                                        header_obj.get("value").and_then(|v| v.as_str())
                                    ) {
                                        lua_script.push_str(&format!(
                                            "  request_handle:headers():add(\"{}\", \"{}\")\n", 
                                            key, value
                                        ));
                                    }
                                }
                            }
                        }
                        
                        // Remove request headers
                        if let Some(headers_to_remove) = filter.config.get("request_headers_to_remove").and_then(|v| v.as_array()) {
                            for header in headers_to_remove {
                                if let Some(header_name) = header.as_str() {
                                    lua_script.push_str(&format!(
                                        "  request_handle:headers():remove(\"{}\")\n", 
                                        header_name
                                    ));
                                }
                            }
                        }
                        
                        lua_script.push_str("end\n\n");
                        lua_script.push_str("function envoy_on_response(response_handle)\n");
                        
                        // Add response headers
                        if let Some(headers_to_add) = filter.config.get("response_headers_to_add").and_then(|v| v.as_array()) {
                            for header in headers_to_add {
                                if let Some(header_obj) = header.get("header") {
                                    if let (Some(key), Some(value)) = (
                                        header_obj.get("key").and_then(|k| k.as_str()),
                                        header_obj.get("value").and_then(|v| v.as_str())
                                    ) {
                                        lua_script.push_str(&format!(
                                            "  response_handle:headers():add(\"{}\", \"{}\")\n", 
                                            key, value
                                        ));
                                    }
                                }
                            }
                        }
                        
                        // Remove response headers
                        if let Some(headers_to_remove) = filter.config.get("response_headers_to_remove").and_then(|v| v.as_array()) {
                            for header in headers_to_remove {
                                if let Some(header_name) = header.as_str() {
                                    lua_script.push_str(&format!(
                                        "  response_handle:headers():remove(\"{}\")\n", 
                                        header_name
                                    ));
                                }
                            }
                        }
                        
                        lua_script.push_str("end\n");
                        
                        // Create Envoy Lua filter configuration
                        let lua_config = envoy_types::pb::envoy::extensions::filters::http::lua::v3::Lua {
                            default_source_code: Some(
                                envoy_types::pb::envoy::config::core::v3::DataSource {
                                    specifier: Some(
                                        envoy_types::pb::envoy::config::core::v3::data_source::Specifier::InlineString(lua_script)
                                    ),
                                    watched_directory: None,
                                }
                            ),
                            ..Default::default()
                        };

                        let mut buf = Vec::new();
                        lua_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
                            resource_type: "Lua".to_string(),
                            source: e,
                        })?;

                        envoy_filters.push(HttpFilter {
                            name: "envoy.filters.http.lua".to_string(),
                            config_type: Some(
                                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                                    EnvoyAny {
                                        type_url: "type.googleapis.com/envoy.extensions.filters.http.lua.v3.Lua".to_string(),
                                        value: buf,
                                    }
                                )
                            ),
                            ..Default::default()
                        });
                    }
                    "authentication" => {
                        info!("Converting authentication filter '{}' to Envoy JWT Auth", filter.name);
                        
                        // Validate JWT configuration before processing
                        Self::validate_jwt_auth_config(filter)?;
                        
                        // For authentication, we'll implement a basic JWT authentication filter
                        // This is a simplified version - in production you'd want more sophisticated auth
                        
                        let jwt_secret = filter.config.get("jwt_secret")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty() && s.len() >= 32) // Minimum security requirements
                            .ok_or_else(|| ConversionError::ValidationFailed {
                                reason: format!(
                                    "JWT secret for filter '{}' must be at least 32 characters long and cannot be empty", 
                                    filter.name
                                )
                            })?;
                            
                        let jwt_issuer = filter.config.get("jwt_issuer")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty() && s.len() <= 100) // Reasonable length limit
                            .ok_or_else(|| ConversionError::ValidationFailed {
                                reason: format!(
                                    "JWT issuer for filter '{}' must be non-empty and at most 100 characters", 
                                    filter.name
                                )
                            })?;
                        
                        // Create JWT authentication configuration
                        let jwt_config = envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::JwtAuthentication {
                            providers: std::collections::HashMap::from([(
                                "default_provider".to_string(),
                                envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::JwtProvider {
                                    issuer: jwt_issuer.to_string(),
                                    jwt_cache_config: Some(envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::JwtCacheConfig {
                                        jwt_cache_size: 1000,
                                        ..Default::default()
                                    }),
                                    jwks_source_specifier: Some(
                                        envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::jwt_provider::JwksSourceSpecifier::LocalJwks(
                                            envoy_types::pb::envoy::config::core::v3::DataSource {
                                                specifier: Some(envoy_types::pb::envoy::config::core::v3::data_source::Specifier::InlineString(
                                                    format!(r#"{{"keys":[{{"kty":"oct","k":"{}"}}]}}"#, 
                                                        base64::prelude::BASE64_STANDARD.encode(jwt_secret))
                                                )),
                                                watched_directory: None,
                                            }
                                        )
                                    ),
                                    ..Default::default()
                                }
                            )]),
                            rules: vec![
                                envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::RequirementRule {
                                    r#match: Some(envoy_types::pb::envoy::config::route::v3::RouteMatch {
                                        path_specifier: Some(
                                            envoy_types::pb::envoy::config::route::v3::route_match::PathSpecifier::Prefix("/".to_string())
                                        ),
                                        ..Default::default()
                                    }),
                                    requirement_type: Some(
                                        envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::requirement_rule::RequirementType::Requires(
                                            envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::JwtRequirement {
                                                requires_type: Some(
                                                    envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::jwt_requirement::RequiresType::ProviderName(
                                                        "default_provider".to_string()
                                                    )
                                                ),
                                            }
                                        )
                                    ),
                                    ..Default::default()
                                }
                            ],
                            ..Default::default()
                        };

                        let mut buf = Vec::new();
                        jwt_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
                            resource_type: "JwtAuthentication".to_string(),
                            source: e,
                        })?;

                        envoy_filters.push(HttpFilter {
                            name: "envoy.filters.http.jwt_authn".to_string(),
                            config_type: Some(
                                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                                    EnvoyAny {
                                        type_url: "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication".to_string(),
                                        value: buf,
                                    }
                                )
                            ),
                            ..Default::default()
                        });
                    }
                    "request_validation" => {
                        info!("Converting request_validation filter '{}' to Envoy RBAC", filter.name);
                        
                        // For request validation, we'll use Envoy's RBAC filter
                        // This can validate requests based on headers, paths, methods, etc.
                        
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

                        // Create RBAC policy for request validation
                        let mut policies = std::collections::HashMap::new();
                        
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

                        let rbac_config = envoy_types::pb::envoy::extensions::filters::http::rbac::v3::Rbac {
                            rules: Some(envoy_types::pb::envoy::config::rbac::v3::Rbac {
                                action: envoy_types::pb::envoy::config::rbac::v3::rbac::Action::Allow as i32,
                                policies,
                                ..Default::default()
                            }),
                            ..Default::default()
                        };

                        let mut buf = Vec::new();
                        rbac_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
                            resource_type: "RBAC".to_string(),
                            source: e,
                        })?;

                        envoy_filters.push(HttpFilter {
                            name: "envoy.filters.http.rbac".to_string(),
                            config_type: Some(
                                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                                    EnvoyAny {
                                        type_url: "type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC".to_string(),
                                        value: buf,
                                    }
                                )
                            ),
                            ..Default::default()
                        });
                    }
                    _ => {
                        warn!("Unknown filter type '{}' for filter '{}'", filter.filter_type, filter.name);
                    }
                }
            }
        }

        // Always add router filter last
        let router_config = Router::default();
        let mut buf = Vec::new();
        router_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
            resource_type: "Router".to_string(),
            source: e,
        })?;

        envoy_filters.push(HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            config_type: Some(
                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                    EnvoyAny {
                        type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router".to_string(),
                        value: buf,
                    }
                )
            ),
            ..Default::default()
        });

        Ok(envoy_filters)
    }

    /// Convert to Envoy Listener protobuf with HTTP filters integration
    pub fn listeners_to_proto(store: &crate::storage::ConfigStore) -> Result<Vec<Any>, ConversionError> {
        // Load config with fallback mechanism
        let app_config = Self::load_config_with_fallback()?;

        info!("Listeners conversion: Creating main listener with HTTP filters");

        // Get all HTTP filters from store
        let http_filters = store.list_http_filters();
        let http_filters: Vec<InternalHttpFilter> = http_filters.iter().map(|f| (**f).clone()).collect();

        // Convert HTTP filters to Envoy format
        let envoy_http_filters = Self::convert_http_filters(
            http_filters,
            &app_config.control_plane.http_filters.default_order
        )?;

        // Create HTTP Connection Manager with filters
        let http_conn_manager = HttpConnectionManager {
            stat_prefix: app_config.envoy_generation.http_filters.stat_prefix.clone(),
            route_specifier: Some(
                envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_connection_manager::RouteSpecifier::Rds(
                    Rds {
                        config_source: Some(
                            envoy_types::pb::envoy::config::core::v3::ConfigSource {
                                config_source_specifier: Some(
                                    envoy_types::pb::envoy::config::core::v3::config_source::ConfigSourceSpecifier::Ads(
                                        envoy_types::pb::envoy::config::core::v3::AggregatedConfigSource {
                                            ..Default::default()
                                        }
                                    )
                                ),
                                resource_api_version: envoy_types::pb::envoy::config::core::v3::ApiVersion::V3 as i32,
                                ..Default::default()
                            }
                        ),
                        route_config_name: app_config.envoy_generation.naming.route_config_name.clone(),
                        ..Default::default()
                    }
                )
            ),
            http_filters: envoy_http_filters,
            ..Default::default()
        };

        // Encode HTTP Connection Manager
        let mut hcm_buf = Vec::new();
        http_conn_manager.encode(&mut hcm_buf).map_err(|e| ConversionError::ProtobufEncoding {
            resource_type: "HttpConnectionManager".to_string(),
            source: e,
        })?;

        // Create main listener
        let listener = Listener {
            name: app_config.envoy_generation.bootstrap.main_listener_name.clone(),
            address: Some(Address {
                address: Some(
                    envoy_types::pb::envoy::config::core::v3::address::Address::SocketAddress(
                        SocketAddress {
                            protocol: Self::protocol_to_proto(&app_config.envoy_generation.cluster.default_protocol),
                            address: app_config.envoy_generation.listener.binding_address.clone(),
                            port_specifier: Some(
                                envoy_types::pb::envoy::config::core::v3::socket_address::PortSpecifier::PortValue(
                                    app_config.envoy_generation.listener.default_port as u32
                                )
                            ),
                            ..Default::default()
                        }
                    )
                )
            }),
            filter_chains: vec![FilterChain {
                filters: vec![Filter {
                    name: app_config.envoy_generation.http_filters.hcm_filter_name.clone(),
                    config_type: Some(
                        envoy_types::pb::envoy::config::listener::v3::filter::ConfigType::TypedConfig(
                            EnvoyAny {
                                type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
                                value: hcm_buf,
                            }
                        )
                    ),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Encode listener to protobuf
        let mut listener_buf = Vec::new();
        listener.encode(&mut listener_buf).map_err(|e| ConversionError::ProtobufEncoding {
            resource_type: "Listener".to_string(),
            source: e,
        })?;

        info!("Listeners conversion: Encoded {} bytes for listener", listener_buf.len());

        Ok(vec![Any {
            type_url: "type.googleapis.com/envoy.config.listener.v3.Listener".to_string(),
            value: listener_buf,
        }])
    }

    /// Get resources by type URL following the Go control plane pattern
    pub fn get_resources_by_type(
        type_url: &str,
        store: &crate::storage::ConfigStore,
    ) -> Result<Vec<Any>, ConversionError> {
        match type_url {
            "type.googleapis.com/envoy.config.cluster.v3.Cluster" => {
                let cluster_list = store.list_clusters();
                Self::clusters_to_proto(cluster_list.iter().map(|c| (**c).clone()).collect())
            }

            "type.googleapis.com/envoy.config.route.v3.RouteConfiguration" => {
                let route_list = store.list_routes();
                Self::routes_to_proto(route_list.iter().map(|r| (**r).clone()).collect())
            }

            "type.googleapis.com/envoy.config.listener.v3.Listener" => {
                Self::listeners_to_proto(store)
            }

            // For other types (endpoints, etc.) return empty for now
            _ => {
                info!("Unsupported resource type: {type_url}");
                Ok(vec![])
            }
        }
    }

    /// Convert DNS lookup family string to Envoy protobuf enum
    fn dns_lookup_family_to_proto(dns_family: &str) -> i32 {
        use envoy_types::pb::envoy::config::cluster::v3::cluster::DnsLookupFamily;
        match dns_family {
            "V4_ONLY" => DnsLookupFamily::V4Only as i32,
            "V6_ONLY" => DnsLookupFamily::V6Only as i32,
            "AUTO" => DnsLookupFamily::Auto as i32,
            _ => {
                warn!("Unknown DNS lookup family '{}', defaulting to V4_ONLY", dns_family);
                DnsLookupFamily::V4Only as i32
            }
        }
    }

    /// Convert protocol string to Envoy protobuf enum
    fn protocol_to_proto(protocol: &str) -> i32 {
        use envoy_types::pb::envoy::config::core::v3::socket_address::Protocol;
        match protocol {
            "TCP" => Protocol::Tcp as i32,
            "UDP" => Protocol::Udp as i32,
            _ => {
                warn!("Unknown protocol '{}', defaulting to TCP", protocol);
                Protocol::Tcp as i32
            }
        }
    }
}
