use dashmap::DashMap;
use std::sync::Arc;

use super::models::{Cluster, Route};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    routes: Arc<DashMap<String, Route>>,
    clusters: Arc<DashMap<String, Cluster>>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
            clusters: Arc::new(DashMap::new()),
        }
    }

    // Route operations
    pub fn add_route(&self, route: Route) -> String {
        let id = route.id.clone();
        self.routes.insert(id.clone(), route);
        id
    }

    pub fn get_route(&self, id: &str) -> Option<Route> {
        self.routes.get(id).map(|r| r.clone())
    }

    pub fn list_routes(&self) -> Vec<Route> {
        self.routes.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn remove_route(&self, id: &str) -> Option<Route> {
        self.routes.remove(id).map(|(_, route)| route)
    }

    // Cluster operations
    pub fn add_cluster(&self, cluster: Cluster) -> String {
        let name = cluster.name.clone();
        self.clusters.insert(name.clone(), cluster);
        name
    }

    pub fn get_cluster(&self, name: &str) -> Option<Cluster> {
        self.clusters.get(name).map(|c| c.clone())
    }

    pub fn list_clusters(&self) -> Vec<Cluster> {
        self.clusters.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn remove_cluster(&self, name: &str) -> Option<Cluster> {
        self.clusters.remove(name).map(|(_, cluster)| cluster)
    }
}