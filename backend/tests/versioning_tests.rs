#![allow(clippy::uninlined_format_args)]

use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;

use envoy_control_plane::storage::{models::*, ConfigStore};
use envoy_control_plane::xds::simple_server::SimpleXdsServer;

#[tokio::test]
#[serial]
async fn test_version_increment_on_cluster_change() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Add a cluster
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster.clone());

    // Simulate version increment (this would normally be triggered by the REST API)
    xds_server.increment_version();

    // Verify the cluster was added
    assert_eq!(store.list_clusters().len(), 1);
}

#[tokio::test]
#[serial]
async fn test_version_increment_on_route_change() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Add a route
    let route = Route {
        id: "test-route-id".to_string(),
        path: "/api/v1/test".to_string(),
        cluster_name: "test-cluster".to_string(),
        prefix_rewrite: Some("/test".to_string()),
        http_methods: None,
    };

    store.add_route(route.clone());

    // Simulate version increment
    xds_server.increment_version();

    // Verify the route was added
    assert_eq!(store.list_routes().len(), 1);
}

#[tokio::test]
#[serial]
async fn test_multiple_version_increments() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test multiple version increments
    for i in 0..5 {
        let cluster = Cluster {
            name: format!("service-{}", i),
            endpoints: vec![Endpoint {
                host: "127.0.0.1".to_string(),
                port: 8080 + i,
            }],
            lb_policy: None, // Use default
        };

        store.add_cluster(cluster);
        xds_server.increment_version();
    }

    // Verify all clusters were added
    assert_eq!(store.list_clusters().len(), 5);
}

#[tokio::test]
#[serial]
async fn test_push_notification_broadcast() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test that increment_version sends broadcast notification
    // This test verifies that the method completes without panicking
    xds_server.increment_version();

    // Give some time for the broadcast to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Test passes if no panic occurred
}

#[tokio::test]
#[serial]
async fn test_concurrent_version_increments() {
    let store = ConfigStore::new();
    let xds_server = Arc::new(SimpleXdsServer::new(store.clone()));

    // Test concurrent version increments
    let handles = (0..10)
        .map(|i| {
            let xds_server = xds_server.clone();
            let store = store.clone();
            tokio::spawn(async move {
                let cluster = Cluster {
                    name: format!("concurrent-service-{}", i),
                    endpoints: vec![Endpoint {
                        host: "127.0.0.1".to_string(),
                        port: 8080 + i,
                    }],
                    lb_policy: None, // Use default
                };

                store.add_cluster(cluster);
                xds_server.increment_version();
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
#[serial]
async fn test_resource_deletion_with_version_increment() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Add a cluster
    let cluster = Cluster {
        name: "deletion-test".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    let cluster_name = cluster.name.clone();
    store.add_cluster(cluster);
    xds_server.increment_version();

    // Verify it was added
    assert_eq!(store.list_clusters().len(), 1);

    // Remove the cluster
    store.remove_cluster(&cluster_name);
    xds_server.increment_version();

    // Verify it was removed
    assert_eq!(store.list_clusters().len(), 0);
}

#[tokio::test]
#[serial]
async fn test_mixed_resource_operations() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Add a cluster
    let cluster = Cluster {
        name: "mixed-service".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster);
    xds_server.increment_version();

    // Add a route
    let route = Route {
        id: "mixed-route".to_string(),
        path: "/mixed".to_string(),
        cluster_name: "mixed-service".to_string(),
        prefix_rewrite: None,
        http_methods: None,
    };

    store.add_route(route);
    xds_server.increment_version();

    // Verify both were added
    assert_eq!(store.list_clusters().len(), 1);
    assert_eq!(store.list_routes().len(), 1);
}

#[tokio::test]
#[serial]
async fn test_notification_with_no_receivers() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test that increment_version works even when no receivers are listening
    // This simulates the case where no Envoy instances are connected
    xds_server.increment_version();

    // Give time for the broadcast attempt
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Should not panic or cause issues
    // Test passes if no panic occurred
}

#[tokio::test]
#[serial]
async fn test_rapid_version_increments() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test rapid version increments
    for _ in 0..100 {
        xds_server.increment_version();
    }

    // Should handle rapid increments without issues
    // Test passes if no panic occurred
}

#[tokio::test]
#[serial]
async fn test_version_increment_with_empty_store() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Test version increment with empty store
    xds_server.increment_version();

    // Should work fine even with no resources
    assert_eq!(store.list_clusters().len(), 0);
    assert_eq!(store.list_routes().len(), 0);
}

#[tokio::test]
#[serial]
async fn test_store_persistence_across_increments() {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());

    // Add resources
    let cluster = Cluster {
        name: "persistent-service".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    store.add_cluster(cluster);

    // Multiple version increments
    for _ in 0..5 {
        xds_server.increment_version();
    }

    // Verify resources persist
    assert_eq!(store.list_clusters().len(), 1);
    assert_eq!(store.list_clusters()[0].name, "persistent-service");
}
