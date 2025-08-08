use crate::storage::models::{
    Cluster as InternalCluster, LoadBalancingPolicy, Route as InternalRoute,
};
use prost::Message;
use prost_types::Any;
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
use envoy_types::pb::envoy::r#type::matcher::v3::RegexMatcher;

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
                    // Single method - use exact match
                    vec![HeaderMatcher {
                        name: ":method".to_string(),
                        header_match_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::ExactMatch(methods[0].clone())
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

    /// Get resources by type URL following the Go control plane pattern
    pub fn get_resources_by_type(
        type_url: &str,
        store: &crate::storage::ConfigStore,
    ) -> Result<Vec<Any>, ConversionError> {
        match type_url {
            "type.googleapis.com/envoy.config.cluster.v3.Cluster" => {
                let cluster_list = store.list_clusters();
                Self::clusters_to_proto(cluster_list)
            }

            "type.googleapis.com/envoy.config.route.v3.RouteConfiguration" => {
                let route_list = store.list_routes();
                Self::routes_to_proto(route_list)
            }

            // For other types (listeners, endpoints, etc.) return empty for now
            // This matches the Go control plane pattern where unsupported types return empty
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
