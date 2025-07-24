use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::AppConfig;
use crate::storage::{Cluster, ConfigStore, Route};

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
    /// Generate Envoy bootstrap configuration using our config system
    pub fn generate_bootstrap_config(app_config: &AppConfig) -> anyhow::Result<String> {
        let bootstrap_yaml = format!(
            r#"node:
  id: {}
  cluster: {}

# Configure Envoy to get config dynamically from our control plane
dynamic_resources:
  # Use ADS (Aggregated Discovery Service) to get all config from one endpoint
  ads_config:
    api_type: GRPC
    transport_api_version: V3
    grpc_services:
    - envoy_grpc:
        cluster_name: {}
    set_node_on_first_message_only: true
  
  # Tell Envoy to get clusters via ADS
  cds_config:
    ads: {{}}
    resource_api_version: V3

static_resources:
  # Define how to connect to our control plane
  clusters:
  - name: {}
    type: {}
    lb_policy: ROUND_ROBIN
    http2_protocol_options: {{}}  # Enable HTTP/2 for gRPC
    load_assignment:
      cluster_name: {}
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address:
                address: {}
                port_value: {}
    connect_timeout: {}s
    
  # Define the main listener that will proxy client requests
  listeners:
  - name: {}
    address:
      socket_address:
        protocol: {}
        address: {}
        port_value: {}
    filter_chains:
    - filters:
      - name: {}
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: {}
          # Get routes dynamically from our control plane
          rds:
            config_source:
              ads: {{}}
              resource_api_version: V3
            route_config_name: {}
          http_filters:
          - name: {}
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router

# Enable admin interface for debugging
admin:
  address:
    socket_address:
      protocol: TCP
      address: {}
      port_value: {}
"#,
            app_config.envoy_generation.bootstrap.node_id,
            app_config.envoy_generation.bootstrap.node_cluster,
            app_config
                .envoy_generation
                .bootstrap
                .control_plane_cluster_name,
            app_config
                .envoy_generation
                .bootstrap
                .control_plane_cluster_name,
            app_config.envoy_generation.cluster.discovery_type,
            app_config
                .envoy_generation
                .bootstrap
                .control_plane_cluster_name,
            app_config.envoy_generation.bootstrap.control_plane_host,
            app_config.control_plane.server.xds_port,
            app_config.envoy_generation.cluster.connect_timeout_seconds,
            app_config.envoy_generation.bootstrap.main_listener_name,
            app_config.envoy_generation.cluster.default_protocol,
            app_config.envoy_generation.listener.binding_address,
            app_config.envoy_generation.listener.default_port,
            app_config.envoy_generation.http_filters.hcm_filter_name,
            app_config.envoy_generation.http_filters.stat_prefix,
            app_config.envoy_generation.naming.route_config_name,
            app_config.envoy_generation.http_filters.router_filter_name,
            app_config.envoy_generation.admin.host,
            app_config.envoy_generation.admin.port,
        );

        Ok(bootstrap_yaml)
    }
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
                        address: app_config.envoy_generation.admin.host.clone(),
                        port_value: app_config.envoy_generation.admin.port,
                    },
                },
            },
            static_resources: StaticResources {
                listeners: vec![Self::create_listener(routes, proxy_port, app_config)?],
                clusters: Self::create_clusters(clusters, app_config)?,
            },
        };

        Ok(envoy_config)
    }

    fn create_listener(
        routes: Vec<Route>,
        port: u16,
        app_config: &AppConfig,
    ) -> anyhow::Result<Listener> {
        let envoy_routes: Vec<EnvoyRoute> = routes
            .into_iter()
            .map(|route| EnvoyRoute {
                route_match: RouteMatch { prefix: route.path },
                route: RouteAction {
                    cluster: route.cluster_name,
                    prefix_rewrite: route.prefix_rewrite,
                },
            })
            .collect();

        let virtual_host = VirtualHost {
            name: app_config.envoy_generation.naming.virtual_host_name.clone(),
            domains: app_config.envoy_generation.naming.default_domains.clone(),
            routes: envoy_routes,
        };

        let http_conn_manager = HttpConnectionManager {
            type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
            stat_prefix: app_config.envoy_generation.http_filters.stat_prefix.clone(),
            route_config: RouteConfiguration {
                name: app_config.envoy_generation.naming.route_config_name.clone(),
                virtual_hosts: vec![virtual_host],
            },
            http_filters: vec![HttpFilter {
                name: app_config.envoy_generation.http_filters.router_filter_name.clone(),
                typed_config: Some(RouterConfig {
                    type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router".to_string(),
                }),
            }],
        };

        Ok(Listener {
            name: app_config.envoy_generation.naming.listener_name.clone(),
            address: SocketAddress {
                socket_address: SocketAddressInner {
                    address: app_config.envoy_generation.listener.binding_address.clone(),
                    port_value: port,
                },
            },
            filter_chains: vec![FilterChain {
                filters: vec![Filter {
                    name: app_config
                        .envoy_generation
                        .http_filters
                        .hcm_filter_name
                        .clone(),
                    typed_config: http_conn_manager,
                }],
            }],
        })
    }

    fn create_clusters(
        clusters: Vec<Cluster>,
        app_config: &AppConfig,
    ) -> anyhow::Result<Vec<EnvoyCluster>> {
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
                    cluster_type: app_config.envoy_generation.cluster.discovery_type.clone(),
                    lb_policy: "ROUND_ROBIN".to_string(),
                    load_assignment: ClusterLoadAssignment {
                        cluster_name: cluster.name,
                        endpoints: vec![LocalityLbEndpoints { lb_endpoints }],
                    },
                })
            })
            .collect()
    }

    pub fn write_config_to_file(config: &EnvoyConfig, file_path: &Path) -> anyhow::Result<()> {
        let yaml_content = serde_yaml::to_string(config)?;
        fs::write(file_path, yaml_content)?;
        Ok(())
    }
}
