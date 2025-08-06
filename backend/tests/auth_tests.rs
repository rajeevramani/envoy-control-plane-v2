#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode, HeaderValue};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use envoy_control_plane::api::routes::create_router;
use envoy_control_plane::auth::JwtKeys;
use envoy_control_plane::config::AuthenticationConfig;
use envoy_control_plane::rbac::RbacEnforcer;
use envoy_control_plane::storage::ConfigStore;
use envoy_control_plane::xds::simple_server::SimpleXdsServer;

/// Helper to create test app with authentication ENABLED
async fn create_auth_enabled_app() -> (Router, ConfigStore) {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());
    
    // Create auth components with authentication ENABLED for auth tests
    let auth_config = AuthenticationConfig {
        enabled: true,  // 🔑 Key: ENABLED for auth tests!
        jwt_secret: "test-auth-secret-key".to_string(),
        jwt_expiry_hours: 1,
        jwt_issuer: "test-auth-issuer".to_string(),
        password_hash_cost: 4, // Low cost for fast tests
    };
    let jwt_keys = JwtKeys::new(auth_config);
    
    // Create RBAC enforcer with default policies
    let rbac = RbacEnforcer::new_simple().await.unwrap();
    
    let app = create_router(store.clone(), xds_server, jwt_keys, rbac);
    (app, store)
}

/// Helper to create test app with authentication DISABLED
async fn create_auth_disabled_app() -> (Router, ConfigStore) {
    let store = ConfigStore::new();
    let xds_server = SimpleXdsServer::new(store.clone());
    
    let auth_config = AuthenticationConfig {
        enabled: false,  // Authentication disabled
        jwt_secret: "test-secret-key".to_string(),
        jwt_expiry_hours: 1,
        jwt_issuer: "test-issuer".to_string(),
        password_hash_cost: 4,
    };
    let jwt_keys = JwtKeys::new(auth_config);
    let rbac = RbacEnforcer::new_simple().await.unwrap();
    
    let app = create_router(store.clone(), xds_server, jwt_keys, rbac);
    (app, store)
}

/// Helper to extract JWT token from login response
fn extract_token_from_response(body: &str) -> String {
    let response: Value = serde_json::from_str(body).expect("Invalid JSON response");
    response["data"]["token"]
        .as_str()
        .expect("No token in response")
        .to_string()
}

// ===========================================
// Authentication Health & Status Tests
// ===========================================

#[tokio::test]
async fn test_auth_health_endpoint() {
    let (app, _store) = create_auth_enabled_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/health")
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
    
    // Should show auth is enabled
    assert!(body_str.contains("authentication_enabled"));
    assert!(body_str.contains("true"));
    assert!(body_str.contains("test-auth-issuer"));
}

#[tokio::test]
async fn test_auth_health_when_disabled() {
    let (app, _store) = create_auth_disabled_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/health")
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
    
    // Should show auth is disabled
    assert!(body_str.contains("authentication_enabled"));
    assert!(body_str.contains("false"));
}

// ===========================================
// Login & JWT Token Tests
// ===========================================

#[tokio::test]
async fn test_login_with_valid_credentials() {
    let (app, _store) = create_auth_enabled_app().await;

    // Login with admin credentials
    let login_data = json!({
        "username": "admin",
        "password": "secure-admin-123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    
    // Should return success with JWT token
    assert!(body_str.contains("success"));
    assert!(body_str.contains("token"));
    assert!(body_str.contains("admin"));
    assert!(body_str.contains("expires_in"));
    
    // Token should have 3 parts (header.payload.signature)
    let token = extract_token_from_response(body_str);
    assert_eq!(token.matches('.').count(), 2);
}

#[tokio::test]
async fn test_login_with_invalid_username() {
    let (app, _store) = create_auth_enabled_app().await;

    let login_data = json!({
        "username": "nonexistent",
        "password": "any_password"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_with_invalid_password() {
    let (app, _store) = create_auth_enabled_app().await;

    let login_data = json!({
        "username": "admin",
        "password": "wrong_password"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_when_auth_disabled() {
    let (app, _store) = create_auth_disabled_app().await;

    let login_data = json!({
        "username": "admin",
        "password": "secure-admin-123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return service unavailable when auth is disabled
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_login_with_all_demo_users() {
    let (app, _store) = create_auth_enabled_app().await;

    // Test all demo users
    let demo_users = vec![
        ("admin", "secure-admin-123"),
        ("user", "secure-user-456"),
        ("demo", "secure-demo-789"),
    ];

    for (username, password) in demo_users {
        let login_data = json!({
            "username": username,
            "password": password
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(login_data.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "Login failed for user: {}", username);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        assert!(body_str.contains("token"), "No token for user: {}", username);
    }
}

// ===========================================
// Protected Route Access Tests
// ===========================================

#[tokio::test]
async fn test_protected_route_without_token() {
    let (app, _store) = create_auth_enabled_app().await;

    // Try to create a route without authentication
    let route_data = json!({
        "path": "/test",
        "cluster_name": "test-cluster"
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

    // Should be unauthorized without token
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_protected_route_with_invalid_token() {
    let (app, _store) = create_auth_enabled_app().await;

    let route_data = json!({
        "path": "/test",
        "cluster_name": "test-cluster"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .header("authorization", "Bearer invalid_token_here")
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be unauthorized with invalid token
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_protected_route_with_valid_admin_token() {
    let (app, _store) = create_auth_enabled_app().await;

    // First, login to get a token
    let login_data = json!({
        "username": "admin",
        "password": "secure-admin-123"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), StatusCode::OK);

    let login_body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_body_str = std::str::from_utf8(&login_body).unwrap();
    let token = extract_token_from_response(login_body_str);

    // Now try to create a route with the token
    let route_data = json!({
        "path": "/api/test",
        "cluster_name": "test-cluster"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Admin should be able to create routes
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("success"));
    assert!(body_str.contains("Route created successfully"));
}

#[tokio::test]
async fn test_user_cannot_create_routes() {
    let (app, _store) = create_auth_enabled_app().await;

    // Login as regular user
    let login_data = json!({
        "username": "user",
        "password": "secure-user-456"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let login_body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_body_str = std::str::from_utf8(&login_body).unwrap();
    let token = extract_token_from_response(login_body_str);

    // Try to create a route (should be forbidden for regular user)
    let route_data = json!({
        "path": "/api/forbidden",
        "cluster_name": "test-cluster"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .method("POST")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(route_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Regular user should be forbidden from creating routes
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ===========================================
// Public Route Access Tests
// ===========================================

#[tokio::test]
async fn test_public_routes_work_without_auth() {
    let (app, _store) = create_auth_enabled_app().await;

    // Test health endpoint (should be public)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Test supported HTTP methods (should be public)  
    let response = app
        .oneshot(
            Request::builder()
                .uri("/supported-http-methods")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_read_routes_work_with_and_without_auth() {
    let (app, store) = create_auth_enabled_app().await;

    // Add a route directly to storage for testing
    use envoy_control_plane::storage::models::Route;
    let route = Route {
        name: "test-route-id".to_string(),
        path: "/test".to_string(),
        cluster_name: "test-cluster".to_string(),
        prefix_rewrite: None,
        http_methods: None,
    };
    store.add_route(route);

    // Test reading routes without authentication (should work)
    let response = app
        .clone()
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
    assert!(body_str.contains("test-route-id"));

    // Now test with authentication (should also work)
    let login_data = json!({
        "username": "user",
        "password": "secure-user-456"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let login_body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_body_str = std::str::from_utf8(&login_body).unwrap();
    let token = extract_token_from_response(login_body_str);

    let response_with_auth = app
        .oneshot(
            Request::builder()
                .uri("/routes")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response_with_auth.status(), StatusCode::OK);
}

// ===========================================
// User Info & JWT Claims Tests
// ===========================================

#[tokio::test]
async fn test_get_user_info_with_valid_token() {
    let (app, _store) = create_auth_enabled_app().await;

    // Login first
    let login_data = json!({
        "username": "admin",
        "password": "secure-admin-123"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let login_body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_body_str = std::str::from_utf8(&login_body).unwrap();
    let token = extract_token_from_response(login_body_str);

    // Get user info
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/me")
                .header("authorization", format!("Bearer {}", token))
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
    
    // Should contain user info and roles
    assert!(body_str.contains("admin"));
    assert!(body_str.contains("user_id"));
    assert!(body_str.contains("roles"));
}

#[tokio::test]
async fn test_get_user_info_without_token() {
    let (app, _store) = create_auth_enabled_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be unauthorized without token
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ===========================================
// Logout Tests
// ===========================================

#[tokio::test]
async fn test_logout_endpoint() {
    let (app, _store) = create_auth_enabled_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/logout")
                .method("POST")
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
    assert!(body_str.contains("Logout successful"));
}