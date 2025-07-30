#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

use envoy_control_plane::api::routes::create_router;
use envoy_control_plane::storage::{models::*, ConfigStore};
use envoy_control_plane::xds::simple_server::SimpleXdsServer;

/// Helper function to create a test app with fresh storage
fn create_test_app() -> (Router, ConfigStore) {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());
    let app = create_router(store.clone(), xds_server);
    (app, store)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _store) = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert_eq!(body_str, "OK");
}

#[tokio::test]
async fn test_create_and_get_cluster() {
    let (app, _store) = create_test_app();

    // Create a cluster
    let cluster_data = json!({
        "name": "test-cluster",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get the cluster
    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("test-cluster"));
    assert!(body_str.contains("127.0.0.1"));
}

#[tokio::test]
async fn test_create_and_get_route() {
    let (app, _store) = create_test_app();

    // Create a route
    let route_data = json!({
        "path": "/api/v1/users",
        "cluster_name": "user-service",
        "prefix_rewrite": "/users"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get the route
    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("/api/v1/users"));
    assert!(body_str.contains("user-service"));
}

#[tokio::test]
async fn test_delete_cluster() {
    let (app, store) = create_test_app();

    // Create a cluster first
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![Endpoint {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }],
        lb_policy: None, // Use default
    };

    let cluster_name = cluster.name.clone();
    store.add_cluster(cluster);

    // Delete the cluster
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/clusters/{}", cluster_name))
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify it's deleted
    let clusters = store.list_clusters();
    assert!(clusters.is_empty());
}

#[tokio::test]
async fn test_delete_route() {
    let (app, store) = create_test_app();

    // Create a route first
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        path: "/test".to_string(),
        cluster_name: "test-cluster".to_string(),
        prefix_rewrite: Some("/test".to_string()),
        http_methods: None,
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Delete the route
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", route_id))
                .method("DELETE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify it's deleted
    let routes = store.list_routes();
    assert!(routes.is_empty());
}

#[tokio::test]
async fn test_invalid_cluster_creation() {
    let (app, _store) = create_test_app();

    // Create a cluster with invalid data (missing name)
    let cluster_data = json!({
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_invalid_route_creation() {
    let (app, _store) = create_test_app();

    // Create a route with invalid data (missing path)
    let route_data = json!({
        "cluster_name": "user-service",
        "prefix_rewrite": "/users"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_get_nonexistent_cluster() {
    let (app, _store) = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_nonexistent_route() {
    let (app, _store) = create_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_multiple_clusters() {
    let (app, _store) = create_test_app();

    // Create first cluster
    let cluster1_data = json!({
        "name": "cluster1",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster1_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Create second cluster
    let cluster2_data = json!({
        "name": "cluster2",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8081
            }
        ]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster2_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get all clusters
    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("cluster1"));
    assert!(body_str.contains("cluster2"));
}

// Tests for our new load balancing policy functionality
#[tokio::test]
async fn test_create_cluster_with_valid_lb_policy() {
    let (app, _store) = create_test_app();

    // Create a cluster with valid LB policy
    let cluster_data = json!({
        "name": "lb-test-cluster",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ],
        "lb_policy": "LEAST_REQUEST"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
    assert!(body_str.contains("Cluster created successfully"));
}

#[tokio::test]
async fn test_create_cluster_with_invalid_lb_policy() {
    let (app, _store) = create_test_app();

    // Create a cluster with invalid LB policy
    let cluster_data = json!({
        "name": "invalid-lb-cluster",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ],
        "lb_policy": "INVALID_POLICY"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Invalid load balancing policy"));
    assert!(body_str.contains("INVALID_POLICY"));
}

#[tokio::test]
async fn test_create_cluster_without_lb_policy_uses_default() {
    let (app, _store) = create_test_app();

    // Create a cluster without specifying LB policy (should use default)
    let cluster_data = json!({
        "name": "default-lb-cluster",
        "endpoints": [
            {
                "host": "127.0.0.1",
                "port": 8080
            }
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clusters")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(cluster_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
}

// Tests for HTTP method routing functionality
#[tokio::test]
async fn test_create_route_with_single_http_method() {
    let (app, _store) = create_test_app();

    // Create a route with single HTTP method
    let route_data = json!({
        "path": "/api/users",
        "cluster_name": "user-service",
        "http_methods": ["GET"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
    assert!(body_str.contains("Route created successfully"));
}

#[tokio::test]
async fn test_create_route_with_multiple_http_methods() {
    let (app, _store) = create_test_app();

    // Create a route with multiple HTTP methods
    let route_data = json!({
        "path": "/api/users",
        "cluster_name": "user-service",
        "http_methods": ["GET", "POST", "PUT"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
}

#[tokio::test]
async fn test_create_route_without_http_methods() {
    let (app, _store) = create_test_app();

    // Create a route without HTTP methods (should accept all methods)
    let route_data = json!({
        "path": "/api/all-methods",
        "cluster_name": "all-service"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
}

#[tokio::test]
async fn test_create_route_with_invalid_http_method() {
    let (app, _store) = create_test_app();

    // Create a route with invalid HTTP method
    let route_data = json!({
        "path": "/api/invalid",
        "cluster_name": "invalid-service",
        "http_methods": ["INVALID_METHOD"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Invalid HTTP method"));
    assert!(body_str.contains("INVALID_METHOD"));
}

#[tokio::test]
async fn test_create_route_with_all_valid_http_methods() {
    let (app, _store) = create_test_app();

    // Create a route with all valid HTTP methods
    let route_data = json!({
        "path": "/api/all-verbs",
        "cluster_name": "verb-service",
        "http_methods": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
}

#[tokio::test]
async fn test_create_route_with_mixed_valid_invalid_methods() {
    let (app, _store) = create_test_app();

    // Create a route with mix of valid and invalid methods
    let route_data = json!({
        "path": "/api/mixed",
        "cluster_name": "mixed-service",
        "http_methods": ["GET", "INVALID", "POST"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Invalid HTTP method"));
    assert!(body_str.contains("INVALID"));
}

#[tokio::test]
async fn test_create_route_with_case_insensitive_methods() {
    let (app, _store) = create_test_app();

    // Create a route with lowercase HTTP methods (should be accepted)
    let route_data = json!({
        "path": "/api/lowercase",
        "cluster_name": "case-service",
        "http_methods": ["get", "post", "put"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
}

// Tests for route update functionality
#[tokio::test]
async fn test_update_route_with_http_methods() {
    let (app, store) = create_test_app();

    // Create initial route
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        path: "/api/users".to_string(),
        cluster_name: "user-service".to_string(),
        prefix_rewrite: None,
        http_methods: Some(vec!["GET".to_string()]),
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Update route with different HTTP methods
    let update_data = json!({
        "path": "/api/users",
        "cluster_name": "user-service",
        "http_methods": ["GET", "POST", "PUT"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", route_id))
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(update_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
    assert!(body_str.contains("Route updated successfully"));

    // Verify the route was actually updated in storage
    let updated_route = store.get_route(&route_id).unwrap();
    assert_eq!(updated_route.http_methods, Some(vec!["GET".to_string(), "POST".to_string(), "PUT".to_string()]));
}

#[tokio::test]
async fn test_update_route_remove_http_methods() {
    let (app, store) = create_test_app();

    // Create initial route with HTTP methods
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        path: "/api/data".to_string(),
        cluster_name: "data-service".to_string(),
        prefix_rewrite: Some("/v1/data".to_string()),
        http_methods: Some(vec!["GET".to_string(), "POST".to_string()]),
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Update route to remove HTTP methods (accept all methods)
    let update_data = json!({
        "path": "/api/data",
        "cluster_name": "data-service",
        "prefix_rewrite": "/v1/data"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", route_id))
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(update_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify the route HTTP methods were removed
    let updated_route = store.get_route(&route_id).unwrap();
    assert_eq!(updated_route.http_methods, None);
}

#[tokio::test]
async fn test_update_route_with_invalid_http_method() {
    let (app, store) = create_test_app();

    // Create initial route
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        path: "/api/test".to_string(),
        cluster_name: "test-service".to_string(),
        prefix_rewrite: None,
        http_methods: Some(vec!["GET".to_string()]),
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Try to update with invalid HTTP method
    let update_data = json!({
        "path": "/api/test",
        "cluster_name": "test-service",
        "http_methods": ["GET", "INVALID_METHOD"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", route_id))
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(update_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Invalid HTTP method"));
    assert!(body_str.contains("INVALID_METHOD"));

    // Verify the route was not updated
    let unchanged_route = store.get_route(&route_id).unwrap();
    assert_eq!(unchanged_route.http_methods, Some(vec!["GET".to_string()]));
}

#[tokio::test]
async fn test_update_nonexistent_route() {
    let (app, _store) = create_test_app();

    let fake_route_id = uuid::Uuid::new_v4().to_string();
    let update_data = json!({
        "path": "/api/fake",
        "cluster_name": "fake-service"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", fake_route_id))
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(update_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_route_change_cluster_and_path() {
    let (app, store) = create_test_app();

    // Create initial route
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        path: "/api/old".to_string(),
        cluster_name: "old-service".to_string(),
        prefix_rewrite: Some("/old".to_string()),
        http_methods: Some(vec!["GET".to_string()]),
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Update route with new path, cluster, and methods
    let update_data = json!({
        "path": "/api/new",
        "cluster_name": "new-service",
        "prefix_rewrite": "/new",
        "http_methods": ["GET", "POST", "DELETE"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/routes/{}", route_id))
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(update_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify all fields were updated
    let updated_route = store.get_route(&route_id).unwrap();
    assert_eq!(updated_route.path, "/api/new");
    assert_eq!(updated_route.cluster_name, "new-service");
    assert_eq!(updated_route.prefix_rewrite, Some("/new".to_string()));
    assert_eq!(updated_route.http_methods, Some(vec!["GET".to_string(), "POST".to_string(), "DELETE".to_string()]));
    // Ensure the ID remains the same
    assert_eq!(updated_route.id, route_id);
}
