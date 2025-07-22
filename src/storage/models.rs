use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Load balancing policy for clusters
/// Hybrid approach: known policies as variants + Custom for flexibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoadBalancingPolicy {
    RoundRobin,
    LeastRequest,
    Random,
    RingHash,
    Custom(String), // For new/unknown policies
}

impl LoadBalancingPolicy {
    /// Convert enum to Envoy's expected string format
    pub fn to_envoy_string(&self) -> String {
        match self {
            LoadBalancingPolicy::RoundRobin => "ROUND_ROBIN".to_string(),
            LoadBalancingPolicy::LeastRequest => "LEAST_REQUEST".to_string(),
            LoadBalancingPolicy::Random => "RANDOM".to_string(),
            LoadBalancingPolicy::RingHash => "RING_HASH".to_string(),
            LoadBalancingPolicy::Custom(policy) => policy.clone(),
        }
    }
    
    /// Parse string from config/API into enum
    pub fn from_str(s: &str) -> Self {
        match s {
            "ROUND_ROBIN" => LoadBalancingPolicy::RoundRobin,
            "LEAST_REQUEST" => LoadBalancingPolicy::LeastRequest,
            "RANDOM" => LoadBalancingPolicy::Random,
            "RING_HASH" => LoadBalancingPolicy::RingHash,
            custom => LoadBalancingPolicy::Custom(custom.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub name: String,
    pub endpoints: Vec<Endpoint>,
    pub lb_policy: Option<LoadBalancingPolicy>, // Optional: falls back to config default
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Route {
    pub fn new(path: String, cluster_name: String, prefix_rewrite: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            path,
            cluster_name,
            prefix_rewrite,
        }
    }
}

impl Cluster {
    pub fn new(name: String, endpoints: Vec<Endpoint>) -> Self {
        Self { 
            name, 
            endpoints, 
            lb_policy: None // Will use default from config
        }
    }
    
    pub fn with_lb_policy(name: String, endpoints: Vec<Endpoint>, lb_policy: LoadBalancingPolicy) -> Self {
        Self { 
            name, 
            endpoints, 
            lb_policy: Some(lb_policy)
        }
    }
}

impl Endpoint {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}
