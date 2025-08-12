#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

use envoy_control_plane::api::routes::create_router;
use envoy_control_plane::auth::JwtKeys;
use envoy_control_plane::config::{AuthenticationConfig, AppConfig, *};
use envoy_control_plane::rbac::RbacEnforcer;
use envoy_control_plane::storage::ConfigStore;
use envoy_control_plane::xds::simple_server::SimpleXdsServer;
use std::path::PathBuf;
use std::sync::Arc;

/// Create a test configuration for integration tests
fn create_test_config() -> AppConfig {
    AppConfig {
        control_plane: ControlPlaneConfig {
            server: ServerConfig {
                rest_port: 8080,
                xds_port: 18000,
                host: "0.0.0.0".to_string(),
            },
            tls: TlsConfig {
                cert_path: "./certs/server.crt".to_string(),
                key_path: "./certs/server.key".to_string(),
                enabled: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            load_balancing: LoadBalancingConfig {
                envoy_version: "1.24".to_string(),
                available_policies: vec!["ROUND_ROBIN".to_string()],
                default_policy: "ROUND_ROBIN".to_string(),
            },
            http_methods: HttpMethodsConfig {
                supported_methods: vec![
                    "GET".to_string(),
                    "POST".to_string(),
                    "PUT".to_string(),
                    "DELETE".to_string(),
                ],
            },
            authentication: AuthenticationConfig {
                enabled: false,  // Will be overridden in individual test functions
                jwt_secret: "test-secret-1234567890abcdefghijklmnopqrstuvwxyz".to_string(),
                jwt_expiry_hours: 24,
                jwt_issuer: "envoy-control-plane-test".to_string(),
                password_hash_cost: 8,
            },
            storage: StorageConfig::default(),
            http_filters: HttpFiltersFeatureConfig::default(),
        },
        envoy_generation: EnvoyGenerationConfig {
            config_dir: PathBuf::from("./configs"),
            admin: AdminConfig {
                host: "127.0.0.1".to_string(),
                port: 9901,
            },
            listener: ListenerConfig {
                binding_address: "0.0.0.0".to_string(),
                default_port: 10000,
            },
            cluster: ClusterConfig {
                connect_timeout_seconds: 5,
                discovery_type: "STRICT_DNS".to_string(),
                dns_lookup_family: "V4_ONLY".to_string(),
                default_protocol: "TCP".to_string(),
            },
            naming: NamingConfig {
                listener_name: "listener_0".to_string(),
                virtual_host_name: "local_service".to_string(),
                route_config_name: "local_route".to_string(),
                default_domains: vec!["*".to_string()],
            },
            bootstrap: BootstrapConfig {
                node_id: "envoy-test-node".to_string(),
                node_cluster: "envoy-test-cluster".to_string(),
                control_plane_host: "control-plane".to_string(),
                main_listener_name: "main_listener".to_string(),
                control_plane_cluster_name: "control_plane_cluster".to_string(),
            },
            http_filters: HttpFiltersConfig {
                stat_prefix: "ingress_http".to_string(),
                router_filter_name: "envoy.filters.http.router".to_string(),
                hcm_filter_name: "envoy.filters.network.http_connection_manager".to_string(),
            },
        },
    }
}

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
    
    let config = Arc::new(create_test_config());
    let app = create_router(store.clone(), xds_server, jwt_keys, rbac, config);
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
    
    let config = Arc::new(create_test_config());
    let app = create_router(store.clone(), xds_server, jwt_keys, rbac, config);
    (app, store)
}

/// Helper to extract auth cookie value from login response headers
fn extract_auth_cookie_from_response(response: &axum::http::Response<axum::body::Body>) -> Option<String> {
    response.headers()
        .get("set-cookie")
        .and_then(|cookie| cookie.to_str().ok())
        .and_then(|cookie_str| {
            if cookie_str.contains("auth_token=") {
                // Extract just the cookie value (JWT token) from "auth_token=<jwt_token>; HttpOnly; ..."
                cookie_str.split(';')
                    .find(|part| part.trim().starts_with("auth_token="))
                    .and_then(|part| {
                        // Split on = and get the value part
                        part.split('=').nth(1).map(|s| s.trim().to_string())
                    })
            } else {
                None
            }
        })
}

/// Helper to perform login and get auth cookie for tests
async fn login_and_get_cookie(app: axum::Router, username: &str, password: &str) -> Result<String, String> {
    let login_data = json!({
        "username": username,
        "password": password
    });

    let login_response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(login_data.to_string()))
                .unwrap(),
        )
        .await
        .map_err(|e| format!("Login request failed: {}", e))?;

    if login_response.status() != StatusCode::OK {
        return Err(format!("Login failed with status: {}", login_response.status()));
    }

    // Extract the auth cookie value (JWT token)
    extract_auth_cookie_from_response(&login_response)
        .ok_or_else(|| "No auth cookie found in login response".to_string())
}

/// Helper to create request with auth cookie
fn create_authenticated_request(method: &str, uri: &str, auth_cookie: &str, body: axum::body::Body) -> Request<axum::body::Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("cookie", format!("auth_token={}", auth_cookie))
        .body(body)
        .unwrap()
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
    
    // Should return success with user info (token is now in httpOnly cookie)
    assert!(body_str.contains("success"));
    assert!(body_str.contains("admin"));
    assert!(body_str.contains("expires_in"));
    assert!(body_str.contains("secure cookie"));
    
    // With httpOnly cookies, the token field should be empty for security
    let response: serde_json::Value = serde_json::from_str(body_str).unwrap();
    assert_eq!(response["data"]["token"].as_str().unwrap(), "");
    assert_eq!(response["data"]["user_id"].as_str().unwrap(), "admin");
    assert_eq!(response["data"]["username"].as_str().unwrap(), "admin");
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

    // First, login to get auth cookie
    let auth_cookie = login_and_get_cookie(app.clone(), "admin", "secure-admin-123")
        .await
        .expect("Failed to login and get auth cookie");

    // Now try to create a route with the auth cookie
    let route_data = json!({
        "name": "api-test-route",
        "path": "/api/test",
        "cluster_name": "test-cluster"
    });

    let response = app
        .oneshot(create_authenticated_request(
            "POST",
            "/routes",
            &auth_cookie,
            Body::from(route_data.to_string())
        ))
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

    // Login as regular user to get auth cookie
    let auth_cookie = login_and_get_cookie(app.clone(), "user", "secure-user-456")
        .await
        .expect("Failed to login and get auth cookie");

    // Try to create a route (should be forbidden for regular user)
    let route_data = json!({
        "name": "forbidden-route",
        "path": "/api/forbidden",
        "cluster_name": "test-cluster"
    });

    let response = app
        .oneshot(create_authenticated_request(
            "POST",
            "/routes",
            &auth_cookie,
            Body::from(route_data.to_string())
        ))
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
    let auth_cookie = login_and_get_cookie(app.clone(), "user", "secure-user-456")
        .await
        .expect("Failed to login and get auth cookie");

    let response_with_auth = app
        .oneshot(create_authenticated_request(
            "GET",
            "/routes",
            &auth_cookie,
            Body::empty()
        ))
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

    // Login first to get auth cookie
    let auth_cookie = login_and_get_cookie(app.clone(), "admin", "secure-admin-123")
        .await
        .expect("Failed to login and get auth cookie");

    // Get user info using auth cookie
    let response = app
        .oneshot(create_authenticated_request(
            "GET",
            "/auth/me",
            &auth_cookie,
            Body::empty()
        ))
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