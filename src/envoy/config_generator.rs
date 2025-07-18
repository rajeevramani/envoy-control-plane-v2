use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::config::AppConfig;
use crate::storage::{ConfigStore, Route, Cluster};

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvoyConfig {
    pub admin: AdminConfig,
    pub static_resources: StaticResources,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminConfig {
    pub address: SocketAddress,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StaticResources {
    pub listeners: Vec<Listener>,
    pub clusters: Vec<EnvoyCluster>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Listener {
    pub name: String,
    pub address: SocketAddress,
    pub filter_chains: Vec<FilterChain>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SocketAddress {
    pub socket_address: SocketAddressInner,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SocketAddressInner {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterChain {
    pub filters: Vec<Filter>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    pub typed_config: HttpConnectionManager,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HttpConnectionManager {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub stat_prefix: String,
    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VirtualHost {
    pub name: String,
    pub domains: Vec<String>,
    pub routes: Vec<EnvoyRoute>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvoyRoute {
    #[serde(rename = "match")]
    pub route_match: RouteMatch,
    pub route: RouteAction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RouteMatch {
    pub prefix: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RouteAction {
    pub cluster: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_rewrite: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HttpFilter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_config: Option<RouterConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RouterConfig {
    #[serde(rename = "@type")]
    pub type_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvoyCluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: String,
    pub lb_policy: String,
    pub load_assignment: ClusterLoadAssignment,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterLoadAssignment {
    pub cluster_name: String,
    pub endpoints: Vec<LocalityLbEndpoints>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalityLbEndpoints {
    pub lb_endpoints: Vec<LbEndpoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LbEndpoint {
    pub endpoint: Endpoint,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub address: SocketAddress,
}

pub struct ConfigGenerator;

impl ConfigGenerator {
    pub fn generate_config(
        store: &ConfigStore,
        app_config: &AppConfig,
        proxy_port: u16,
    ) -> anyhow::Result<EnvoyConfig> {
        let routes = store.list_routes();
        let clusters = store.list_clusters();

        let envoy_config = EnvoyConfig {
            admin: AdminConfig {
                address: SocketAddress {
                    socket_address: SocketAddressInner {
                        address: "127.0.0.1".to_string(),
                        port_value: app_config.envoy.admin_port,
                    },
                },
            },
            static_resources: StaticResources {
                listeners: vec![Self::create_listener(routes, proxy_port)?],
                clusters: Self::create_clusters(clusters)?,
            },
        };

        Ok(envoy_config)
    }

    fn create_listener(routes: Vec<Route>, port: u16) -> anyhow::Result<Listener> {
        let envoy_routes: Vec<EnvoyRoute> = routes
            .into_iter()
            .map(|route| EnvoyRoute {
                route_match: RouteMatch {
                    prefix: route.path,
                },
                route: RouteAction {
                    cluster: route.cluster_name,
                    prefix_rewrite: route.prefix_rewrite,
                },
            })
            .collect();

        let virtual_host = VirtualHost {
            name: "local_service".to_string(),
            domains: vec!["*".to_string()],
            routes: envoy_routes,
        };

        let http_conn_manager = HttpConnectionManager {
            type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
            stat_prefix: "ingress_http".to_string(),
            route_config: RouteConfiguration {
                name: "local_route".to_string(),
                virtual_hosts: vec![virtual_host],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: Some(RouterConfig {
                    type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router".to_string(),
                }),
            }],
        };

        Ok(Listener {
            name: "listener_0".to_string(),
            address: SocketAddress {
                socket_address: SocketAddressInner {
                    address: "0.0.0.0".to_string(),
                    port_value: port,
                },
            },
            filter_chains: vec![FilterChain {
                filters: vec![Filter {
                    name: "envoy.filters.network.http_connection_manager".to_string(),
                    typed_config: http_conn_manager,
                }],
            }],
        })
    }

    fn create_clusters(clusters: Vec<Cluster>) -> anyhow::Result<Vec<EnvoyCluster>> {
        clusters
            .into_iter()
            .map(|cluster| {
                let lb_endpoints: Vec<LbEndpoint> = cluster
                    .endpoints
                    .into_iter()
                    .map(|endpoint| LbEndpoint {
                        endpoint: Endpoint {
                            address: SocketAddress {
                                socket_address: SocketAddressInner {
                                    address: endpoint.host,
                                    port_value: endpoint.port,
                                },
                            },
                        },
                    })
                    .collect();

                Ok(EnvoyCluster {
                    name: cluster.name.clone(),
                    cluster_type: "STRICT_DNS".to_string(),
                    lb_policy: "ROUND_ROBIN".to_string(),
                    load_assignment: ClusterLoadAssignment {
                        cluster_name: cluster.name,
                        endpoints: vec![LocalityLbEndpoints { lb_endpoints }],
                    },
                })
            })
            .collect()
    }

    pub fn write_config_to_file(
        config: &EnvoyConfig,
        file_path: &Path,
    ) -> anyhow::Result<()> {
        let yaml_content = serde_yaml::to_string(config)?;
        fs::write(file_path, yaml_content)?;
        Ok(())
    }
}