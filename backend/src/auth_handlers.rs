use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use bcrypt::{hash, verify, BcryptError};

use crate::api::handlers::ApiResponse;
use crate::api::routes::AppState;
use crate::auth::{create_jwt_token, Claims};

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Password hashing failed")]
    HashingFailed(#[from] BcryptError),
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("User not found")]
    UserNotFound,
}

impl From<AuthError> for StatusCode {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::HashingFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::InvalidCredentials | AuthError::UserNotFound => StatusCode::UNAUTHORIZED,
        }
    }
}

/// Hash a password using bcrypt with async support
pub async fn hash_password(password: &str, cost: u32) -> Result<String, BcryptError> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash(password, cost))
        .await
        .map_err(|_| BcryptError::InvalidCost(cost.to_string()))?
}

/// Verify a password against a bcrypt hash with async support
pub async fn verify_password(password: &str, hash: &str) -> Result<bool, BcryptError> {
    let password = password.to_string();
    let hash = hash.to_string();
    let hash_clone = hash.clone();
    tokio::task::spawn_blocking(move || verify(password, &hash))
        .await
        .map_err(|_| BcryptError::InvalidHash(hash_clone))?
}

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

/// User struct with secure bcrypt password hashing
#[derive(Debug)]
struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String, // bcrypt hashed password
}

impl User {
    /// Create a new user with bcrypt hashed password
    async fn new(id: &str, username: &str, password: &str, cost: u32) -> Result<Self, AuthError> {
        let password_hash = hash_password(password, cost).await?;
        Ok(Self {
            id: id.to_string(),
            username: username.to_string(),
            password_hash,
        })
    }
    
    /// Verify password against bcrypt hash
    async fn verify_password(&self, password: &str) -> Result<bool, AuthError> {
        verify_password(password, &self.password_hash)
            .await
            .map_err(AuthError::HashingFailed)
    }
}

/// Get demo users with bcrypt hashed passwords (in production, this would query a database)
async fn get_demo_users(cost: u32) -> Result<Vec<User>, AuthError> {
    Ok(vec![
        User::new("admin", "admin", "admin123", cost).await?,
        User::new("user", "user", "user123", cost).await?,
        User::new("demo", "demo", "demo123", cost).await?,
        User::new("developer", "developer", "password123", cost).await?,
    ])
}

/// Login endpoint - authenticates user and returns JWT token
pub async fn login(
    State(app_state): State<AppState>,
    Json(login_req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
    tracing::info!("🔐 Login attempt for user: {}", login_req.username);

    // Check if authentication is enabled
    if !app_state.jwt_keys.config.enabled {
        tracing::warn!("⚠️  Authentication is disabled");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Find user in our demo database
    let users = get_demo_users(app_state.jwt_keys.config.password_hash_cost)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load demo users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    let user = users
        .iter()
        .find(|u| u.username == login_req.username)
        .ok_or_else(|| {
            tracing::info!("❌ User '{}' not found", login_req.username);
            StatusCode::UNAUTHORIZED
        })?;

    // Verify password using bcrypt
    match user.verify_password(&login_req.password).await {
        Ok(true) => {
            tracing::info!("✅ Password verification successful for user: {}", user.username);
        },
        Ok(false) => {
            tracing::info!("❌ Invalid password for user '{}'", login_req.username);
            return Err(StatusCode::UNAUTHORIZED);
        },
        Err(e) => {
            tracing::error!("Password verification failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Create JWT token
    let token = create_jwt_token(
        user.id.clone(),
        user.username.clone(),
        &app_state.jwt_keys.config,
    )
    .map_err(|e| {
        tracing::error!("❌ Failed to create JWT token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("✅ Login successful for user: {}", user.username);

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
    tracing::info!("🔍 Getting user info for: {}", claims.user_id());

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
    tracing::info!("👋 User logged out");
    
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
        "password_hash_cost": app_state.jwt_keys.config.password_hash_cost,
        "available_demo_users": ["admin", "user", "demo", "developer"],
        "security_note": "Passwords are now securely hashed with bcrypt"
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

    #[tokio::test]
    async fn test_password_hashing_and_verification() {
        let password = "test_password_123";
        let hash = hash_password(password, 8).await.unwrap(); // Lower cost for tests
        
        // Verify correct password
        assert!(verify_password(password, &hash).await.unwrap());
        
        // Verify incorrect password
        assert!(!verify_password("wrong_password", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_user_password_verification_bcrypt() {
        let user = User::new("test", "testuser", "password123", 8).await.unwrap();
        
        assert!(user.verify_password("password123").await.unwrap());
        assert!(!user.verify_password("wrong_password").await.unwrap());
    }

    #[tokio::test]
    async fn test_demo_users_creation() {
        let users = get_demo_users(8).await.unwrap(); // Lower cost for tests
        assert_eq!(users.len(), 4);
        assert!(users.iter().any(|u| u.username == "admin"));
        assert!(users.iter().any(|u| u.username == "user"));
        assert!(users.iter().any(|u| u.username == "demo"));
        assert!(users.iter().any(|u| u.username == "developer"));
    }

    #[tokio::test]
    async fn test_bcrypt_error_handling() {
        // Test invalid hash format
        let result = verify_password("test", "invalid_hash").await;
        assert!(result.is_err());
        
        // Test invalid cost (too high)
        let result = hash_password("test", 32).await;
        assert!(result.is_err());
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