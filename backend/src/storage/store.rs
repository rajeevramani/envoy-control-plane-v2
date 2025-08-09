use dashmap::DashMap;
use std::sync::Arc;

use super::models::{Cluster, Route, HttpFilter, RouteFilters};
use super::StorageError;

#[derive(Debug, Clone)]
pub struct ConfigStore {
    routes: Arc<DashMap<String, Arc<Route>>>,
    clusters: Arc<DashMap<String, Arc<Cluster>>>,
    http_filters: Arc<DashMap<String, Arc<HttpFilter>>>,
    route_filters: Arc<DashMap<String, RouteFilters>>,
    config: crate::config::StorageConfig,
}

impl ConfigStore {
    pub fn with_config(config: crate::config::StorageConfig) -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
            clusters: Arc::new(DashMap::new()),
            http_filters: Arc::new(DashMap::new()),
            route_filters: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Creates a ConfigStore with default configuration for testing
    pub fn new() -> Self {
        let default_config = crate::config::StorageConfig {
            limits: crate::config::StorageLimitsConfig {
                max_routes: 1000,
                max_clusters: 500,
                max_endpoints_per_cluster: 50,
                max_http_filters: 50,
            },
            behavior: crate::config::StorageBehaviorConfig {
                reject_on_capacity: true,
                enable_metrics: true,
            },
        };
        Self::with_config(default_config)
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigStore {
    // Route operations with enhanced error handling
    pub fn add_route(&self, route: Route) -> Result<String, StorageError> {
        // Check capacity before adding
        let current_count = self.routes.len();
        if current_count >= self.config.limits.max_routes {
            if self.config.behavior.reject_on_capacity {
                return Err(StorageError::CapacityExceeded {
                    current: current_count,
                    limit: self.config.limits.max_routes,
                });
            } else {
                // Log warning but allow (for gradual rollout)
                eprintln!("⚠️  Warning: Route capacity approaching limit ({}/{})", 
                         current_count, self.config.limits.max_routes);
            }
        }

        let name = route.name.clone();
        
        // Check for conflicts
        if self.routes.contains_key(&name) {
            return Err(StorageError::ResourceConflict {
                resource_type: "Route".to_string(),
                resource_id: name,
            });
        }

        // Validate route before storing
        self.validate_route(&route)?;

        self.routes.insert(name.clone(), Arc::new(route));
        Ok(name)
    }

    pub fn get_route(&self, id: &str) -> Result<Arc<Route>, StorageError> {
        self.routes.get(id).map(|r| r.clone()).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "Route".to_string(),
                resource_id: id.to_string(),
            }
        })
    }

    pub fn list_routes(&self) -> Vec<Arc<Route>> {
        self.routes
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn remove_route(&self, id: &str) -> Result<Arc<Route>, StorageError> {
        self.routes.remove(id).map(|(_, route)| route).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "Route".to_string(),
                resource_id: id.to_string(),
            }
        })
    }

    pub fn update_route(&self, id: &str, updated_route: Route) -> Result<Arc<Route>, StorageError> {
        // Verify route exists first
        if !self.routes.contains_key(id) {
            return Err(StorageError::ResourceNotFound {
                resource_type: "Route".to_string(),
                resource_id: id.to_string(),
            });
        }

        // Validate updated route
        self.validate_route(&updated_route)?;

        let arc_route = Arc::new(updated_route);
        self.routes.insert(id.to_string(), arc_route.clone());
        Ok(arc_route)
    }

    // Cluster operations with enhanced error handling
    pub fn add_cluster(&self, cluster: Cluster) -> Result<String, StorageError> {
        // Validate cluster capacity
        let current_count = self.clusters.len();
        if current_count >= self.config.limits.max_clusters {
            if self.config.behavior.reject_on_capacity {
                return Err(StorageError::CapacityExceeded {
                    current: current_count,
                    limit: self.config.limits.max_clusters,
                });
            } else {
                eprintln!("⚠️  Warning: Cluster capacity approaching limit ({}/{})", 
                         current_count, self.config.limits.max_clusters);
            }
        }

        // Validate endpoints per cluster
        if cluster.endpoints.len() > self.config.limits.max_endpoints_per_cluster {
            return Err(StorageError::ValidationFailed {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: format!("Too many endpoints: {}/{}", 
                              cluster.endpoints.len(), 
                              self.config.limits.max_endpoints_per_cluster),
            });
        }

        let name = cluster.name.clone();
        
        // Check for conflicts
        if self.clusters.contains_key(&name) {
            return Err(StorageError::ResourceConflict {
                resource_type: "Cluster".to_string(),
                resource_id: name,
            });
        }

        // Validate cluster before storing
        self.validate_cluster(&cluster)?;

        self.clusters.insert(name.clone(), Arc::new(cluster));
        Ok(name)
    }

    pub fn get_cluster(&self, name: &str) -> Result<Arc<Cluster>, StorageError> {
        self.clusters.get(name).map(|c| c.clone()).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "Cluster".to_string(),
                resource_id: name.to_string(),
            }
        })
    }

    pub fn list_clusters(&self) -> Vec<Arc<Cluster>> {
        self.clusters
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn remove_cluster(&self, name: &str) -> Result<Arc<Cluster>, StorageError> {
        self.clusters.remove(name).map(|(_, cluster)| cluster).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "Cluster".to_string(),
                resource_id: name.to_string(),
            }
        })
    }

    /// Update existing cluster with validation
    pub fn update_cluster(&self, name: &str, updated_cluster: Cluster) -> Result<Arc<Cluster>, StorageError> {
        // Verify cluster exists first
        if !self.clusters.contains_key(name) {
            return Err(StorageError::ResourceNotFound {
                resource_type: "Cluster".to_string(),
                resource_id: name.to_string(),
            });
        }

        // Validate endpoints per cluster
        if updated_cluster.endpoints.len() > self.config.limits.max_endpoints_per_cluster {
            return Err(StorageError::ValidationFailed {
                resource_type: "Cluster".to_string(),
                resource_id: updated_cluster.name.clone(),
                reason: format!("Too many endpoints: {}/{}", 
                              updated_cluster.endpoints.len(), 
                              self.config.limits.max_endpoints_per_cluster),
            });
        }

        // Validate updated cluster
        self.validate_cluster(&updated_cluster)?;

        let arc_cluster = Arc::new(updated_cluster);
        self.clusters.insert(name.to_string(), arc_cluster.clone());
        Ok(arc_cluster)
    }

    // Capacity reporting methods for observability
    pub fn get_route_capacity_info(&self) -> (usize, usize, f64) {
        let current = self.routes.len();
        let limit = self.config.limits.max_routes;
        let utilization = (current as f64) / (limit as f64) * 100.0;
        (current, limit, utilization)
    }

    pub fn get_cluster_capacity_info(&self) -> (usize, usize, f64) {
        let current = self.clusters.len();
        let limit = self.config.limits.max_clusters;
        let utilization = (current as f64) / (limit as f64) * 100.0;
        (current, limit, utilization)
    }

    // Validation methods
    fn validate_route(&self, route: &Route) -> Result<(), StorageError> {
        // Basic route validation - can be extended with validation crate integration
        if route.name.is_empty() {
            return Err(StorageError::ValidationFailed {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route name cannot be empty".to_string(),
            });
        }

        if route.path.is_empty() {
            return Err(StorageError::ValidationFailed {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route path cannot be empty".to_string(),
            });
        }

        if route.cluster_name.is_empty() {
            return Err(StorageError::ValidationFailed {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                reason: "Route cluster_name cannot be empty".to_string(),
            });
        }

        // Check if referenced cluster exists
        if !self.clusters.contains_key(&route.cluster_name) {
            return Err(StorageError::DependencyMissing {
                resource_type: "Route".to_string(),
                resource_id: route.name.clone(),
                dependency: format!("Cluster '{}'", route.cluster_name),
            });
        }

        Ok(())
    }

    fn validate_cluster(&self, cluster: &Cluster) -> Result<(), StorageError> {
        // Basic cluster validation
        if cluster.name.is_empty() {
            return Err(StorageError::ValidationFailed {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: "Cluster name cannot be empty".to_string(),
            });
        }

        if cluster.endpoints.is_empty() {
            return Err(StorageError::ValidationFailed {
                resource_type: "Cluster".to_string(),
                resource_id: cluster.name.clone(),
                reason: "Cluster must have at least one endpoint".to_string(),
            });
        }

        // Validate each endpoint
        for (i, endpoint) in cluster.endpoints.iter().enumerate() {
            if endpoint.host.is_empty() {
                return Err(StorageError::ValidationFailed {
                    resource_type: "Cluster".to_string(),
                    resource_id: cluster.name.clone(),
                    reason: format!("Endpoint {} host cannot be empty", i + 1),
                });
            }

            if endpoint.port == 0 || endpoint.port > 65535 {
                return Err(StorageError::ValidationFailed {
                    resource_type: "Cluster".to_string(),
                    resource_id: cluster.name.clone(),
                    reason: format!("Endpoint {} port {} is invalid (must be 1-65535)", i + 1, endpoint.port),
                });
            }
        }

        Ok(())
    }

    // HTTP Filter operations
    pub fn add_http_filter(&self, filter: HttpFilter, supported_filters: &[String]) -> Result<String, StorageError> {
        // Check capacity before adding
        let current_count = self.http_filters.len();
        if current_count >= self.config.limits.max_http_filters {
            if self.config.behavior.reject_on_capacity {
                return Err(StorageError::CapacityExceeded {
                    current: current_count,
                    limit: self.config.limits.max_http_filters,
                });
            } else {
                eprintln!("⚠️  Warning: HTTP filter capacity approaching limit ({}/{})", 
                         current_count, self.config.limits.max_http_filters);
            }
        }

        let name = filter.name.clone();
        
        // Check for conflicts
        if self.http_filters.contains_key(&name) {
            return Err(StorageError::ResourceConflict {
                resource_type: "HttpFilter".to_string(),
                resource_id: name,
            });
        }

        // Validate filter before storing
        filter.validate(supported_filters).map_err(|reason| StorageError::ValidationFailed {
            resource_type: "HttpFilter".to_string(),
            resource_id: filter.name.clone(),
            reason,
        })?;

        self.http_filters.insert(name.clone(), Arc::new(filter));
        Ok(name)
    }

    pub fn get_http_filter(&self, name: &str) -> Result<Arc<HttpFilter>, StorageError> {
        self.http_filters.get(name).map(|f| f.clone()).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "HttpFilter".to_string(),
                resource_id: name.to_string(),
            }
        })
    }

    pub fn list_http_filters(&self) -> Vec<Arc<HttpFilter>> {
        self.http_filters
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn remove_http_filter(&self, name: &str) -> Result<Arc<HttpFilter>, StorageError> {
        // Check if filter is in use by any routes
        for route_filter in self.route_filters.iter() {
            if route_filter.filter_names.contains(&name.to_string()) {
                return Err(StorageError::ValidationFailed {
                    resource_type: "HttpFilter".to_string(),
                    resource_id: name.to_string(),
                    reason: format!("Filter is in use by route: {}", route_filter.route_name),
                });
            }
        }

        self.http_filters.remove(name).map(|(_, filter)| filter).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "HttpFilter".to_string(),
                resource_id: name.to_string(),
            }
        })
    }

    pub fn update_http_filter(&self, name: &str, updated_filter: HttpFilter, supported_filters: &[String]) -> Result<Arc<HttpFilter>, StorageError> {
        // Verify filter exists
        if !self.http_filters.contains_key(name) {
            return Err(StorageError::ResourceNotFound {
                resource_type: "HttpFilter".to_string(),
                resource_id: name.to_string(),
            });
        }

        // Validate updated filter
        updated_filter.validate(supported_filters).map_err(|reason| StorageError::ValidationFailed {
            resource_type: "HttpFilter".to_string(),
            resource_id: updated_filter.name.clone(),
            reason,
        })?;

        let arc_filter = Arc::new(updated_filter);
        self.http_filters.insert(name.to_string(), arc_filter.clone());
        Ok(arc_filter)
    }

    // Route-Filter association operations
    pub fn add_route_filters(&self, route_filters: RouteFilters) -> Result<String, StorageError> {
        let route_name = route_filters.route_name.clone();
        
        // Verify route exists
        if !self.routes.contains_key(&route_name) {
            return Err(StorageError::ResourceNotFound {
                resource_type: "Route".to_string(),
                resource_id: route_name,
            });
        }

        // Get list of existing filter names for validation
        let existing_filters: Vec<String> = self.http_filters
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        // Validate route filters
        route_filters.validate(&existing_filters).map_err(|reason| StorageError::ValidationFailed {
            resource_type: "RouteFilters".to_string(),
            resource_id: route_filters.route_name.clone(),
            reason,
        })?;

        self.route_filters.insert(route_name.clone(), route_filters);
        Ok(route_name)
    }

    pub fn get_route_filters(&self, route_name: &str) -> Option<RouteFilters> {
        self.route_filters.get(route_name).map(|rf| rf.clone())
    }

    pub fn remove_route_filters(&self, route_name: &str) -> Result<RouteFilters, StorageError> {
        self.route_filters.remove(route_name).map(|(_, rf)| rf).ok_or_else(|| {
            StorageError::ResourceNotFound {
                resource_type: "RouteFilters".to_string(),
                resource_id: route_name.to_string(),
            }
        })
    }

    // Capacity reporting for HTTP filters
    pub fn get_http_filter_capacity_info(&self) -> (usize, usize, f64) {
        let current = self.http_filters.len();
        let limit = self.config.limits.max_http_filters;
        let utilization = (current as f64) / (limit as f64) * 100.0;
        (current, limit, utilization)
    }
}
