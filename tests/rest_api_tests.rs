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
    };

    let cluster_name = cluster.name.clone();
    store.add_cluster(cluster);

    // Delete the cluster
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/clusters/{}", cluster_name))
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
    };

    let route_id = route.id.clone();
    store.add_route(route);

    // Delete the route
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/routes/{}", route_id))
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
