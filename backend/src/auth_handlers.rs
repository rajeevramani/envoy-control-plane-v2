use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::api::handlers::ApiResponse;
use crate::api::routes::AppState;
use crate::auth::{create_jwt_token, Claims};

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub expires_in: i64, // seconds until expiration
}

/// User info request (for testing JWT extraction)
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub permissions: std::collections::HashMap<String, Vec<String>>,
}

/// Simple user database (in production, this would be a real database)
/// For demo purposes, we'll have some hardcoded users
struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String, // In production, this would be bcrypt hashed
}

impl User {
    fn new(id: &str, username: &str, password: &str) -> Self {
        // In production, you'd hash the password with bcrypt
        // For demo, we'll use plain text (NOT SECURE!)
        Self {
            id: id.to_string(),
            username: username.to_string(),
            password_hash: password.to_string(), // WARNING: Not hashed for demo!
        }
    }
    
    fn verify_password(&self, password: &str) -> bool {
        // In production, use bcrypt::verify
        self.password_hash == password
    }
}

/// Get demo users (in production, this would query a database)
fn get_demo_users() -> Vec<User> {
    vec![
        User::new("admin", "admin", "admin123"),
        User::new("user", "user", "user123"),
        User::new("demo", "demo", "demo123"),
        User::new("developer", "developer", "password123"),
    ]
}

/// Login endpoint - authenticates user and returns JWT token
pub async fn login(
    State(app_state): State<AppState>,
    Json(login_req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
    println!("🔐 Login attempt for user: {}", login_req.username);

    // Check if authentication is enabled
    if !app_state.jwt_keys.config.enabled {
        println!("⚠️  Authentication is disabled");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Find user in our demo database
    let users = get_demo_users();
    let user = users
        .iter()
        .find(|u| u.username == login_req.username)
        .ok_or_else(|| {
            println!("❌ User '{}' not found", login_req.username);
            StatusCode::UNAUTHORIZED
        })?;

    // Verify password
    if !user.verify_password(&login_req.password) {
        println!("❌ Invalid password for user '{}'", login_req.username);
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Create JWT token
    let token = create_jwt_token(
        user.id.clone(),
        user.username.clone(),
        &app_state.jwt_keys.config,
    )
    .map_err(|e| {
        println!("❌ Failed to create JWT token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    println!("✅ Login successful for user: {}", user.username);

    let response = LoginResponse {
        token,
        user_id: user.id.clone(),
        username: user.username.clone(),
        expires_in: (app_state.jwt_keys.config.jwt_expiry_hours * 3600) as i64,
    };

    Ok(Json(ApiResponse::success(
        response,
        "Login successful",
    )))
}

/// Get current user info from JWT token
/// This endpoint demonstrates how to extract user info from JWT claims
pub async fn get_user_info(
    State(app_state): State<AppState>,
    axum::Extension(claims): axum::Extension<Claims>, // This will be injected by the auth middleware
) -> Result<Json<ApiResponse<UserInfo>>, StatusCode> {
    println!("🔍 Getting user info for: {}", claims.user_id());

    // Get user roles from RBAC system
    let roles = app_state.rbac.get_user_roles(claims.user_id()).await;
    
    // Get user permissions from RBAC system
    let permissions = app_state.rbac.get_user_permissions(claims.user_id()).await;

    let user_info = UserInfo {
        user_id: claims.user_id().to_string(),
        username: claims.username.clone(),
        roles,
        permissions,
    };

    Ok(Json(ApiResponse::success(
        user_info,
        "User info retrieved successfully",
    )))
}

/// Logout endpoint (for completeness - JWT tokens can't be revoked easily)
/// In production, you might maintain a token blacklist
pub async fn logout() -> Json<ApiResponse<()>> {
    println!("👋 User logged out");
    
    // Note: JWTs are stateless, so we can't truly "logout" without maintaining
    // a blacklist or using short-lived tokens with refresh tokens
    Json(ApiResponse::success(
        (), 
        "Logout successful - token will expire naturally"
    ))
}

/// Health check for auth system
pub async fn auth_health(State(app_state): State<AppState>) -> Json<ApiResponse<serde_json::Value>> {
    let status = serde_json::json!({
        "authentication_enabled": app_state.jwt_keys.config.enabled,
        "jwt_issuer": app_state.jwt_keys.config.jwt_issuer,
        "jwt_expiry_hours": app_state.jwt_keys.config.jwt_expiry_hours,
        "available_demo_users": ["admin", "user", "demo"],
        "demo_credentials": {
            "admin": "admin123",
            "user": "user123", 
            "demo": "demo123"
        }
    });

    Json(ApiResponse::success(
        status,
        "Authentication system is healthy",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthenticationConfig;

    fn create_test_config() -> AuthenticationConfig {
        AuthenticationConfig {
            enabled: true,
            jwt_secret: "test-secret-key".to_string(),
            jwt_expiry_hours: 1,
            jwt_issuer: "test-issuer".to_string(),
            password_hash_cost: 8,
        }
    }

    #[test]
    fn test_user_password_verification() {
        let user = User::new("test", "testuser", "password123");
        assert!(user.verify_password("password123"));
        assert!(!user.verify_password("wrong_password"));
    }

    #[test]
    fn test_demo_users_creation() {
        let users = get_demo_users();
        assert_eq!(users.len(), 4);
        assert!(users.iter().any(|u| u.username == "admin"));
        assert!(users.iter().any(|u| u.username == "user"));
        assert!(users.iter().any(|u| u.username == "demo"));
        assert!(users.iter().any(|u| u.username == "developer"));
    }

    #[tokio::test]
    async fn test_jwt_token_creation_in_login() {
        let config = create_test_config();
        let token = create_jwt_token(
            "test_user".to_string(),
            "Test User".to_string(),
            &config,
        ).unwrap();
        
        // JWT has 3 parts separated by dots
        assert_eq!(token.matches('.').count(), 2);
        assert!(token.len() > 50); // Reasonable token length
    }
}