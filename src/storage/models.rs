use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
        Self { name, endpoints }
    }
}

impl Endpoint {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}