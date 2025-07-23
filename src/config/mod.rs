use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub control_plane: ControlPlaneConfig,
    pub envoy_generation: EnvoyGenerationConfig,
}

// Control plane configuration (for our Rust application)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ControlPlaneConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub load_balancing: LoadBalancingConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub rest_port: u16,
    pub xds_port: u16,
    pub host: String,
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

// Envoy configuration generation (for generating Envoy configs)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvoyGenerationConfig {
    pub config_dir: PathBuf,
    pub admin: AdminConfig,
    pub listener: ListenerConfig,
    pub cluster: ClusterConfig,
    pub naming: NamingConfig,
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

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name("config"))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}
