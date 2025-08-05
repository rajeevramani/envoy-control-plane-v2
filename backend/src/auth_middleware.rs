use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;

use crate::auth::JwtKeys;
use crate::rbac::{extract_resource_and_action, RbacEnforcer};

/// Combined authentication and authorization middleware
/// 1. First: JWT authentication (who are you?)
/// 2. Then: RBAC authorization (what can you do?)
/// Supports both Authorization header and httpOnly cookies
pub async fn auth_middleware(
    State((jwt_keys, rbac)): State<(JwtKeys, RbacEnforcer)>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    println!("🛡️  Auth Middleware: Starting authentication & authorization...");
    
    // Skip auth if disabled
    if !jwt_keys.config.enabled {
        println!("⚠️  Auth Middleware: Authentication disabled - allowing request");
        return Ok(next.run(request).await);
    }
    
    // Step 1: Extract JWT token from cookie or Authorization header
    let jwt_token = match jar.get("auth_token") {
        Some(cookie) => {
            println!("🍪 Auth Middleware: Found JWT token in httpOnly cookie");
            cookie.value().to_string()
        }
        None => {
            // Fallback to Authorization header for backward compatibility
            println!("🔍 Auth Middleware: No cookie found, checking Authorization header...");
            request
                .headers()
                .get("authorization")
                .and_then(|header| header.to_str().ok())
                .and_then(|auth_str| {
                    if auth_str.starts_with("Bearer ") {
                        Some(auth_str.strip_prefix("Bearer ")?.to_string())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    println!("❌ Auth Middleware: No JWT token found in cookie or Authorization header");
                    StatusCode::UNAUTHORIZED
                })?
        }
    };
    
    // Step 2: JWT Authentication - Validate JWT token
    println!("🔍 Auth Middleware: Step 1 - JWT Authentication");
    let token_data = jsonwebtoken::decode::<crate::auth::Claims>(
        &jwt_token,
        &jwt_keys.decoding_key,
        &jwt_keys.validation,
    )
    .map_err(|e| {
        println!("❌ Auth Middleware: JWT validation failed: {e}");
        StatusCode::UNAUTHORIZED
    })?;
    
    let claims = token_data.claims;
    
    let user_id = claims.user_id();
    println!("✅ Auth Middleware: JWT valid for user: {user_id}" );
    
    // Step 3: RBAC Authorization - Check permissions
    println!("🔐 Auth Middleware: Step 2 - RBAC Authorization");
    let method = request.method().as_str();
    let path = request.uri().path();
    let (resource, action) = extract_resource_and_action(method, path);
    
    println!("📋 Auth Middleware: Checking permission - method={method}, path={path}, resource={resource}, action={action}");
    
    // Check if user has permission for this resource/action
    let allowed = rbac
        .check_permission(user_id, &resource, &action)
        .await
        .map_err(|e| {
            println!("❌ Auth Middleware: RBAC error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    if !allowed {
        println!("🚫 Auth Middleware: Access DENIED - user '{user_id}' cannot '{action}' on '{resource}'");
        return Err(StatusCode::FORBIDDEN);
    }
    

    println!("✅ Auth Middleware: Access GRANTED - user '{user_id}' can '{action}' on '{resource}'") ;
    
    // Add claims to request extensions for handlers to use
    request.extensions_mut().insert(claims);
    
    // Continue to the actual handler
    Ok(next.run(request).await)
}

/// Optional authentication middleware for routes that should work with or without auth
/// If JWT is present and valid, it adds claims to request
/// If JWT is missing or invalid, it continues without claims
/// Supports both Authorization header and httpOnly cookies
pub async fn optional_auth_middleware(
    State(jwt_keys): State<JwtKeys>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Response {
    println!("🔓 Optional Auth Middleware: Checking for optional authentication...");
    
    // Skip if auth is disabled
    if !jwt_keys.config.enabled {
        println!("⚠️  Optional Auth: Authentication disabled");
        return next.run(request).await;
    }
    
    // Try to extract JWT from cookie or Authorization header
    let jwt_token = match jar.get("auth_token") {
        Some(cookie) => {
            println!("🍪 Optional Auth: Found JWT token in httpOnly cookie");
            Some(cookie.value().to_string())
        }
        None => {
            // Fallback to Authorization header
            println!("🔍 Optional Auth: No cookie found, checking Authorization header...");
            request
                .headers()
                .get("authorization")
                .and_then(|header| header.to_str().ok())
                .and_then(|auth_str| {
                    if auth_str.starts_with("Bearer ") {
                        Some(auth_str.strip_prefix("Bearer ")?.to_string())
                    } else {
                        None
                    }
                })
        }
    };

    if let Some(token) = jwt_token {
        println!("🔍 Optional Auth: Found JWT token, validating...");
        
        if let Ok(token_data) = jsonwebtoken::decode::<crate::auth::Claims>(
            &token,
            &jwt_keys.decoding_key,
            &jwt_keys.validation,
        ) {
            println!("✅ Optional Auth: Valid JWT for user: {}", token_data.claims.user_id());
            request.extensions_mut().insert(token_data.claims);
        } else {
            println!("⚠️  Optional Auth: Invalid JWT token, continuing without authentication");
        }
    } else {
        println!("ℹ️  Optional Auth: No JWT token found, continuing without authentication");
    }
    
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthenticationConfig;
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;
    
    fn create_test_config() -> AuthenticationConfig {
        AuthenticationConfig {
            enabled: true,
            jwt_secret: generate_test_jwt_secret(),
            jwt_expiry_hours: 1,
            jwt_issuer: "test-issuer".to_string(),
            password_hash_cost: 8,
        }
    }
    
    /// Generate a secure random JWT secret for testing
    fn generate_test_jwt_secret() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789-_";
        const SECRET_LEN: usize = 64;
        
        let mut rng = rand::thread_rng();
        (0..SECRET_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
    
    async fn test_handler() -> &'static str {
        "Hello, World!"
    }
    
    #[tokio::test]
    async fn test_optional_auth_middleware_without_token() {
        let config = create_test_config();
        let jwt_keys = JwtKeys::new(config);
        
        // Create a simple test app with optional auth middleware
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(middleware::from_fn_with_state(
                jwt_keys.clone(),
                optional_auth_middleware,
            ))
            .with_state(jwt_keys);
        
        let request = Request::builder()
            .method(Method::GET)
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        
        // Should succeed with optional auth (no token required)
        assert_eq!(response.status(), StatusCode::OK);
    }
}