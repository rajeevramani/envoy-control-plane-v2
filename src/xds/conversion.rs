use crate::storage::models::{Route as InternalRoute, Cluster as InternalCluster, Endpoint as InternalEndpoint};
use prost::Message;
use prost_types::Any;

// Import actual Envoy protobuf types
use envoy_types::pb::envoy::config::cluster::v3::Cluster;
use envoy_types::pb::envoy::config::route::v3::{RouteConfiguration, VirtualHost, Route, RouteMatch, RouteAction};
use envoy_types::pb::envoy::config::core::v3::{Address, SocketAddress};
use envoy_types::pb::envoy::config::endpoint::v3::{ClusterLoadAssignment, LocalityLbEndpoints, LbEndpoint, Endpoint};

// Include the generated protobuf code for ADS
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

pub struct ProtoConverter;

impl ProtoConverter {
    /// Convert internal routes to Envoy RouteConfiguration protobuf
    /// Following the Go control plane pattern from makeRoute()
    pub fn routes_to_proto(routes: Vec<InternalRoute>) -> anyhow::Result<Vec<Any>> {
        if routes.is_empty() {
            return Ok(vec![]);
        }

        println!("✅ Routes conversion: Creating RouteConfiguration with {} routes", routes.len());

        // Create routes following the Go control plane pattern
        let proto_routes: Vec<Route> = routes.into_iter().map(|route| {
            println!("  - Route: {} -> {}", route.path, route.cluster_name);
            
            Route {
                r#match: Some(RouteMatch {
                    path_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_match::PathSpecifier::Prefix(route.path)),
                    ..Default::default()
                }),
                action: Some(envoy_types::pb::envoy::config::route::v3::route::Action::Route(RouteAction {
                    cluster_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_action::ClusterSpecifier::Cluster(route.cluster_name)),
                    prefix_rewrite: route.prefix_rewrite.unwrap_or_default(),
                    ..Default::default()
                })),
                ..Default::default()
            }
        }).collect();

        // Create virtual host with all routes
        let virtual_host = VirtualHost {
            name: "local_service".to_string(),
            domains: vec!["*".to_string()],
            routes: proto_routes,
            ..Default::default()
        };

        // Create RouteConfiguration
        let route_config = RouteConfiguration {
            name: "local_route".to_string(),
            virtual_hosts: vec![virtual_host],
            ..Default::default()
        };

        // Encode to protobuf bytes
        let mut buf = Vec::new();
        route_config.encode(&mut buf)?;
        
        println!("✅ Routes conversion: Encoded {} bytes", buf.len());

        Ok(vec![Any {
            type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
            value: buf,
        }])
    }

    /// Convert internal clusters to Envoy Cluster protobuf
    /// Following the Go control plane pattern from makeCluster() and makeEndpoint()
    pub fn clusters_to_proto(clusters: Vec<InternalCluster>) -> anyhow::Result<Vec<Any>> {
        if clusters.is_empty() {
            return Ok(vec![]);
        }

        println!("✅ Clusters conversion: Creating {} clusters", clusters.len());

        let mut proto_clusters = Vec::new();

        for cluster in clusters {
            let cluster_name = cluster.name.clone(); // Clone before moving
            println!("  - Cluster: {} ({} endpoints)", cluster_name, cluster.endpoints.len());
            
            // Create endpoints following the Go control plane pattern
            let lb_endpoints: Vec<LbEndpoint> = cluster.endpoints.into_iter().map(|endpoint| {
                println!("    - Endpoint: {}:{}", endpoint.host, endpoint.port);
                
                LbEndpoint {
                    host_identifier: Some(envoy_types::pb::envoy::config::endpoint::v3::lb_endpoint::HostIdentifier::Endpoint(
                        Endpoint {
                            address: Some(Address {
                                address: Some(envoy_types::pb::envoy::config::core::v3::address::Address::SocketAddress(
                                    SocketAddress {
                                        protocol: envoy_types::pb::envoy::config::core::v3::socket_address::Protocol::Tcp as i32,
                                        address: endpoint.host,
                                        port_specifier: Some(envoy_types::pb::envoy::config::core::v3::socket_address::PortSpecifier::PortValue(endpoint.port as u32)),
                                        ..Default::default()
                                    }
                                )),
                                ..Default::default()
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
                lb_policy: envoy_types::pb::envoy::config::cluster::v3::cluster::LbPolicy::RoundRobin as i32,
                load_assignment: Some(load_assignment),
                connect_timeout: Some(envoy_types::pb::google::protobuf::Duration { seconds: 5, nanos: 0 }),
                dns_lookup_family: envoy_types::pb::envoy::config::cluster::v3::cluster::DnsLookupFamily::V4Only as i32,
                ..Default::default()
            };

            // Encode to protobuf bytes
            let mut buf = Vec::new();
            proto_cluster.encode(&mut buf)?;
            
            println!("✅ Cluster conversion: Encoded {} bytes for {}", buf.len(), cluster_name);

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
    ) -> anyhow::Result<Vec<Any>> {
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
                println!("ℹ️  Unsupported resource type: {}", type_url);
                Ok(vec![])
            }
        }
    }
}