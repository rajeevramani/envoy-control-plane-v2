use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod validation;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub control_plane: ControlPlaneConfig,
    pub envoy_generation: EnvoyGenerationConfig,
}

// Control plane configuration (for our Rust application)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ControlPlaneConfig {
    pub server: ServerConfig,
    pub tls: TlsConfig,
    pub logging: LoggingConfig,
    pub load_balancing: LoadBalancingConfig,
    pub http_methods: HttpMethodsConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub rest_port: u16,
    pub xds_port: u16,
    pub host: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadBalancingConfig {
    pub envoy_version: String,
    pub available_policies: Vec<String>,
    pub default_policy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpMethodsConfig {
    pub supported_methods: Vec<String>,
}

// Envoy configuration generation (for generating Envoy configs)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvoyGenerationConfig {
    pub config_dir: PathBuf,
    pub admin: AdminConfig,
    pub listener: ListenerConfig,
    pub cluster: ClusterConfig,
    pub naming: NamingConfig,
    pub bootstrap: BootstrapConfig,
    pub http_filters: HttpFiltersConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListenerConfig {
    pub binding_address: String,
    pub default_port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusterConfig {
    pub connect_timeout_seconds: u64,
    pub discovery_type: String,
    pub dns_lookup_family: String,
    pub default_protocol: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NamingConfig {
    pub listener_name: String,
    pub virtual_host_name: String,
    pub route_config_name: String,
    pub default_domains: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapConfig {
    pub node_id: String,
    pub node_cluster: String,
    pub control_plane_host: String,
    pub main_listener_name: String,
    pub control_plane_cluster_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpFiltersConfig {
    pub stat_prefix: String,
    pub router_filter_name: String,
    pub hcm_filter_name: String,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name("config"))
            .build()?;

        let config: Self = settings.try_deserialize()?;

        // Validate the loaded configuration
        validation::validate_config(&config)?;

        Ok(config)
    }

    #[cfg(test)]
    pub fn create_test_config() -> Self {
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
                        "GET".to_string(),
                        "POST".to_string(),
                        "PUT".to_string(),
                        "DELETE".to_string(),
                    ],
                },
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
}
