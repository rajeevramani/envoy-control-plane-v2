use crate::storage::{Route as InternalRoute, Cluster as InternalCluster, Endpoint as InternalEndpoint};
use prost::Message;
use prost_types::Any;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

pub struct ProtoConverter;

impl ProtoConverter {
    pub fn routes_to_proto(routes: Vec<InternalRoute>) -> anyhow::Result<Vec<Any>> {
        let mut proto_routes = Vec::new();
        
        // Group routes by their virtual host (for simplicity, we'll use one virtual host)
        let virtual_host = VirtualHost {
            name: "local_service".to_string(),
            domains: vec!["*".to_string()],
            routes: routes.into_iter().map(|route| {
                Route {
                    r#match: Some(RouteMatch {
                        prefix: route.path,
                    }),
                    route: Some(RouteAction {
                        cluster: route.cluster_name,
                        prefix_rewrite: route.prefix_rewrite.unwrap_or_default(),
                    }),
                }
            }).collect(),
        };

        let route_config = RouteConfiguration {
            name: "local_route".to_string(),
            virtual_hosts: vec![virtual_host],
        };

        let mut buf = Vec::new();
        route_config.encode(&mut buf)?;
        
        proto_routes.push(Any {
            type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
            value: buf,
        });

        Ok(proto_routes)
    }

    pub fn clusters_to_proto(clusters: Vec<InternalCluster>) -> anyhow::Result<Vec<Any>> {
        let mut proto_clusters = Vec::new();

        for cluster in clusters {
            let load_assignment = ClusterLoadAssignment {
                cluster_name: cluster.name.clone(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: cluster.endpoints.into_iter().map(|endpoint| {
                        LbEndpoint {
                            endpoint: Some(Endpoint {
                                address: Some(Address {
                                    socket_address: Some(SocketAddress {
                                        address: endpoint.host,
                                        port_value: endpoint.port as u32,
                                    }),
                                }),
                            }),
                        }
                    }).collect(),
                }],
            };

            let proto_cluster = Cluster {
                name: cluster.name,
                r#type: cluster::DiscoveryType::StrictDns as i32,
                lb_policy: cluster::LbPolicy::RoundRobin as i32,
                load_assignment: Some(load_assignment),
            };

            let mut buf = Vec::new();
            proto_cluster.encode(&mut buf)?;
            
            proto_clusters.push(Any {
                type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
                value: buf,
            });
        }

        Ok(proto_clusters)
    }
}