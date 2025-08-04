use serde::{Deserialize, Serialize};
use std::str::FromStr;
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

impl FromStr for LoadBalancingPolicy {
    type Err = LoadBalancingPolicyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Validate input
        if s.is_empty() {
            return Err(LoadBalancingPolicyParseError::Empty);
        }
        
        // Check for invalid characters that might break configuration
        if s.contains('\n') || s.contains('\r') || s.contains('\0') {
            return Err(LoadBalancingPolicyParseError::InvalidCharacters(s.to_string()));
        }
        
        let policy = match s {
            "ROUND_ROBIN" => LoadBalancingPolicy::RoundRobin,
            "LEAST_REQUEST" => LoadBalancingPolicy::LeastRequest,
            "RANDOM" => LoadBalancingPolicy::Random,
            "RING_HASH" => LoadBalancingPolicy::RingHash,
            custom => LoadBalancingPolicy::Custom(custom.to_string()),
        };
        Ok(policy)
    }
}

/// Errors that can occur when parsing LoadBalancingPolicy
#[derive(Debug, thiserror::Error)]
pub enum LoadBalancingPolicyParseError {
    #[error("Load balancing policy cannot be empty")]
    Empty,
    
    #[error("Load balancing policy contains invalid characters: '{0}'")]
    InvalidCharacters(String),
}

impl LoadBalancingPolicy {
    /// Safe parsing method that provides better error context
    pub fn parse_safe(s: &str, context: &str) -> Result<Self, String> {
        s.parse().map_err(|e: LoadBalancingPolicyParseError| {
            format!("Failed to parse load balancing policy in {context}: {e}")
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
    pub http_methods: Option<Vec<String>>, // GET, POST, PUT, DELETE, etc.
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
            http_methods: None,
        }
    }

    pub fn with_methods(
        path: String,
        cluster_name: String,
        prefix_rewrite: Option<String>,
        http_methods: Option<Vec<String>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            path,
            cluster_name,
            prefix_rewrite,
            http_methods,
        }
    }
}

impl Cluster {
    pub fn with_lb_policy(
        name: String,
        endpoints: Vec<Endpoint>,
        lb_policy: LoadBalancingPolicy,
    ) -> Self {
        Self {
            name,
            endpoints,
            lb_policy: Some(lb_policy),
        }
    }
}

impl Endpoint {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}
