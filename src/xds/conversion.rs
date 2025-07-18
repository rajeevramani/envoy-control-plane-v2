use crate::storage::models::{Route as InternalRoute, Cluster as InternalCluster, Endpoint as InternalEndpoint};
use prost::Message;
use prost_types::Any;
use std::collections::HashMap;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

pub struct ProtoConverter;

impl ProtoConverter {
    /// Convert internal routes to Envoy RouteConfiguration protobuf
    /// Following the Go control plane pattern from makeRoute()
    pub fn routes_to_proto(routes: Vec<InternalRoute>) -> anyhow::Result<Vec<Any>> {
        if routes.is_empty() {
            return Ok(vec![]);
        }

        // Create a single RouteConfiguration with all routes
        // This follows the Go control plane pattern where routes are grouped into virtual hosts
        let route_config = serde_json::json!({
            "name": "local_route",
            "virtual_hosts": [{
                "name": "local_service", 
                "domains": ["*"],
                "routes": routes.into_iter().map(|route| {
                    let mut route_json = serde_json::json!({
                        "match": {
                            "prefix": route.path
                        },
                        "route": {
                            "cluster": route.cluster_name
                        }
                    });
                    
                    // Add prefix_rewrite if specified, following Go control plane pattern
                    if let Some(prefix_rewrite) = route.prefix_rewrite {
                        if !prefix_rewrite.is_empty() {
                            route_json["route"]["prefix_rewrite"] = serde_json::Value::String(prefix_rewrite);
                        }
                    }
                    
                    route_json
                }).collect::<Vec<_>>()
            }]
        });

        let serialized = serde_json::to_vec(&route_config)?;
        
        Ok(vec![Any {
            type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
            value: serialized,
        }])
    }

    /// Convert internal clusters to Envoy Cluster protobuf
    /// Following the Go control plane pattern from makeCluster() and makeEndpoint()
    pub fn clusters_to_proto(clusters: Vec<InternalCluster>) -> anyhow::Result<Vec<Any>> {
        let mut proto_clusters = Vec::new();

        for cluster in clusters {
            // Create cluster configuration following Go control plane pattern
            let cluster_config = serde_json::json!({
                "name": cluster.name,
                "type": "STRICT_DNS",  // Same as Go control plane's LOGICAL_DNS
                "lb_policy": "ROUND_ROBIN",
                "connect_timeout": "5s",
                "dns_lookup_family": "V4_ONLY",
                "load_assignment": {
                    "cluster_name": cluster.name,
                    "endpoints": [{
                        "lb_endpoints": cluster.endpoints.into_iter().map(|endpoint| {
                            serde_json::json!({
                                "endpoint": {
                                    "address": {
                                        "socket_address": {
                                            "protocol": "TCP",
                                            "address": endpoint.host,
                                            "port_value": endpoint.port
                                        }
                                    }
                                }
                            })
                        }).collect::<Vec<_>>()
                    }]
                }
            });

            let serialized = serde_json::to_vec(&cluster_config)?;
            
            proto_clusters.push(Any {
                type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
                value: serialized,
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