#![allow(clippy::uninlined_format_args)]

use envoy_control_plane::storage::{models::*, ConfigStore};
use envoy_control_plane::xds::conversion::ProtoConverter;
use envoy_control_plane::xds::simple_server::SimpleXdsServer;

#[tokio::test]
async fn test_xds_server_creation() {
    let store = ConfigStore::new();
    SimpleXdsServer::new(store.clone());

    // Test that the server was created successfully
    // Test passes if no panic occurred during creation
}

#[tokio::test]
async fn test_get_resources_by_type_clusters() {
    let store = ConfigStore::new();

    // Add a test cluster
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster);

    // Test getting cluster resources
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.cluster.v3.Cluster",
        &store,
    )
    .unwrap();

    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].type_url,
        "type.googleapis.com/envoy.config.cluster.v3.Cluster"
    );
}

#[tokio::test]
async fn test_get_resources_by_type_routes() {
    let store = ConfigStore::new();

    // Add a test route
    let route = Route {
        name: "test-id".to_string(),
        path: "/api/v1/test".to_string(),
        cluster_name: "test-cluster".to_string(),
        prefix_rewrite: Some("/test".to_string()),
        http_methods: None,
    };

    store.add_route(route);

    // Test getting route resources
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.route.v3.RouteConfiguration",
        &store,
    )
    .unwrap();

    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].type_url,
        "type.googleapis.com/envoy.config.route.v3.RouteConfiguration"
    );
}

#[tokio::test]
async fn test_get_resources_unsupported_type() {
    let store = ConfigStore::new();

    // Test getting unsupported resource type
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.listener.v3.Listener",
        &store,
    )
    .unwrap();

    assert!(resources.is_empty());
}

#[tokio::test]
async fn test_version_increment() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test version increment functionality
    xds_server.increment_version();
    xds_server.increment_version();

    // This test just ensures the increment_version method doesn't panic
    // Test passes if no panic occurred
}

#[tokio::test]
async fn test_cluster_storage_and_retrieval() {
    let store = ConfigStore::new();

    // Test adding and retrieving multiple clusters
    let cluster1 = Cluster {
        name: "service1".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    let cluster2 = Cluster {
        name: "service2".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8081,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster1);
    store.add_cluster(cluster2);

    let clusters = store.list_clusters();
    assert_eq!(clusters.len(), 2);

    // Test resource conversion
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.cluster.v3.Cluster",
        &store,
    )
    .unwrap();

    assert_eq!(resources.len(), 2);
}

#[tokio::test]
async fn test_route_storage_and_retrieval() {
    let store = ConfigStore::new();

    // Test adding and retrieving multiple routes
    let route1 = Route {
        name: "route1".to_string(),
        path: "/api/v1/users".to_string(),
        cluster_name: "user-service".to_string(),
        prefix_rewrite: Some("/users".to_string()),
        http_methods: None,
    };

    let route2 = Route {
        name: "route2".to_string(),
        path: "/api/v1/orders".to_string(),
        cluster_name: "order-service".to_string(),
        prefix_rewrite: None,
        http_methods: None,
    };

    store.add_route(route1);
    store.add_route(route2);

    let routes = store.list_routes();
    assert_eq!(routes.len(), 2);

    // Test resource conversion
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.route.v3.RouteConfiguration",
        &store,
    )
    .unwrap();

    assert_eq!(resources.len(), 1); // Routes are consolidated into one RouteConfiguration
}

#[tokio::test]
async fn test_resource_deletion() {
    let store = ConfigStore::new();

    // Add a cluster and route
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    let route = Route {
        name: "test-route-id".to_string(),
        path: "/test".to_string(),
        cluster_name: "test-cluster".to_string(),
        prefix_rewrite: None,
        http_methods: None,
    };

    store.add_cluster(cluster.clone());
    store.add_route(route.clone());

    // Verify they exist
    assert_eq!(store.list_clusters().len(), 1);
    assert_eq!(store.list_routes().len(), 1);

    // Remove them
    store.remove_cluster(&cluster.name);
    store.remove_route(&route.name);

    // Verify they're gone
    assert_eq!(store.list_clusters().len(), 0);
    assert_eq!(store.list_routes().len(), 0);
}

#[tokio::test]
async fn test_concurrent_resource_access() {
    let store = ConfigStore::new();

    // Test concurrent access to the store
    let handles = (0..10)
        .map(|i| {
            let store = store.clone();
            tokio::spawn(async move {
                let cluster = Cluster {
                    name: format!("service-{}", i),
                    endpoints: vec![Endpoint {
                        host: "127.0.0.1".to_string(),
                        port: 8080 + i,
                    }],
                    lb_policy: None, // Use default
                };

                store.add_cluster(cluster);
            })
        })
        .collect::<Vec<_>>();

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all clusters were added
    assert_eq!(store.list_clusters().len(), 10);
}

#[tokio::test]
async fn test_discovery_response_creation() {
    let store = ConfigStore::new();

    // Add a test cluster
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster);

    // Get resources
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.cluster.v3.Cluster",
        &store,
    )
    .unwrap();

    // Validate resources
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].type_url,
        "type.googleapis.com/envoy.config.cluster.v3.Cluster"
    );
    assert!(!resources[0].value.is_empty());
}
