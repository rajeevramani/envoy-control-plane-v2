use super::errors::ConversionError;
use super::utils::{load_config_with_fallback, validate_cluster};
use crate::storage::models::{Cluster as InternalCluster, LoadBalancingPolicy};
use prost::Message;
use prost_types::Any;
use tracing::{info, warn};

// Import Envoy protobuf types for clusters
use envoy_types::pb::envoy::config::cluster::v3::Cluster;
use envoy_types::pb::envoy::config::core::v3::{Address, SocketAddress};
use envoy_types::pb::envoy::config::endpoint::v3::{
    ClusterLoadAssignment, Endpoint, LbEndpoint, LocalityLbEndpoints,
};

/// Convert internal clusters to Envoy protobuf format
pub fn clusters_to_proto(clusters: Vec<InternalCluster>) -> Result<Vec<Any>, ConversionError> {
    if clusters.is_empty() {
        return Ok(vec![]);
    }

    // Load config with fallback mechanism
    let app_config = load_config_with_fallback()?;

    info!(
        "Clusters conversion: Creating {} clusters",
        clusters.len()
    );

    let mut proto_clusters = Vec::new();

    for cluster in clusters {
        // Validate cluster before conversion
        validate_cluster(&cluster)?;
        
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
                                    protocol: protocol_to_proto(&app_config.envoy_generation.cluster.default_protocol),
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

        // Determine load balancing policy
        let lb_policy = match cluster.lb_policy.unwrap_or(LoadBalancingPolicy::RoundRobin) {
            crate::storage::models::LoadBalancingPolicy::RoundRobin => {
                envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::RoundRobin as i32
            }
            crate::storage::models::LoadBalancingPolicy::LeastRequest => {
                envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::LeastRequest as i32
            }
            crate::storage::models::LoadBalancingPolicy::Random => {
                envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::Random as i32
            }
            crate::storage::models::LoadBalancingPolicy::RingHash => {
                envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::RingHash as i32
            }
            crate::storage::models::LoadBalancingPolicy::Custom(_) => {
                warn!("Custom load balancing policy not supported, defaulting to ROUND_ROBIN");
                envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::RoundRobin as i32
            }
        };

        // Create the Envoy cluster
        let envoy_cluster = Cluster {
            name: cluster_name,
            cluster_discovery_type: Some(
                envoy_types::pb::envoy::config::cluster::v3::cluster::ClusterDiscoveryType::Type(
                    discovery_type_to_proto(&app_config.envoy_generation.cluster.discovery_type),
                ),
            ),
            lb_policy,
            load_assignment: Some(load_assignment),
            connect_timeout: Some(envoy_types::pb::google::protobuf::Duration {
                seconds: app_config.envoy_generation.cluster.connect_timeout_seconds as i64,
                nanos: 0,
            }),
            dns_lookup_family: dns_lookup_family_to_proto(&app_config.envoy_generation.cluster.dns_lookup_family),
            ..Default::default()
        };

        // Encode to protobuf Any
        let mut buf = Vec::new();
        envoy_cluster.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
            resource_type: "Cluster".to_string(),
            source: e,
        })?;

        proto_clusters.push(Any {
            type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
            value: buf,
        });
    }

    Ok(proto_clusters)
}

/// Convert discovery type string to Envoy protobuf enum
fn discovery_type_to_proto(discovery_type: &str) -> i32 {
    use envoy_types::pb::envoy::config::cluster::v3::cluster::DiscoveryType;
    match discovery_type {
        "STATIC" => DiscoveryType::Static as i32,
        "STRICT_DNS" => DiscoveryType::StrictDns as i32,
        "LOGICAL_DNS" => DiscoveryType::LogicalDns as i32,
        "EDS" => DiscoveryType::Eds as i32,
        "ORIGINAL_DST" => DiscoveryType::OriginalDst as i32,
        _ => {
            warn!("Unknown discovery type '{}', defaulting to STRICT_DNS", discovery_type);
            DiscoveryType::StrictDns as i32
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