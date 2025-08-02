use axum::{extract::State, 
    http::StatusCode}
;
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::config::AuthenticationConfig;

/// JWT Claims - what goes inside the token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,    // Subject (user ID) - "admin", "user123"
    pub exp: usize,     // Expiration timestamp
    pub iat: usize,     // Issued at timestamp 
    pub iss: String,    // Issuer - "envoy-control-plane"
    pub username: String, // Display name
}

impl Claims {
    pub fn new(user_id: String, username: String, config: &AuthenticationConfig) -> Self {
        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::hours(config.jwt_expiry_hours as i64);
        
        Self {
            sub: user_id,
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            iss: config.jwt_issuer.clone(),
            username,
        }
    }
    
    pub fn user_id(&self) -> &str {
        &self.sub
    }
}

/// JWT validation keys - we'll store this in app state
#[derive(Clone)]
pub struct JwtKeys {
    pub decoding_key: DecodingKey,
    pub encoding_key: EncodingKey,
    pub validation: Validation,
    pub config: AuthenticationConfig,
}

impl JwtKeys {
    pub fn new(config: AuthenticationConfig) -> Self {
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_ref());
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_ref());
        
        let mut validation = Validation::default();
        validation.set_issuer(&[&config.jwt_issuer]);
        
        Self {
            decoding_key,
            encoding_key,
            validation,
            config,
        }
    }
}

/// Modern 2024 pattern: Simple JWT extractor that uses State
/// This is much cleaner and follows standard Axum patterns
pub async fn extract_jwt_claims(
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    State(jwt_keys): State<JwtKeys>,
) -> Result<Claims, StatusCode> {
    println!("🔍 JWT Extractor: Starting token extraction...");
    
    // Check if authentication is enabled
    if !jwt_keys.config.enabled {
        println!("⚠️  JWT Extractor: Authentication disabled");
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    println!("🔑 JWT Extractor: Found Bearer token");
    
    // Validate JWT token
    let token = authorization.token();
    let token_data = decode::<Claims>(token, &jwt_keys.decoding_key, &jwt_keys.validation)
        .map_err(|e| {
            println!("❌ JWT Extractor: Token validation failed: {}", e);
            StatusCode::UNAUTHORIZED
        })?;
        
    println!("✅ JWT Extractor: Token validated for user: {}", token_data.claims.user_id());
    Ok(token_data.claims)
}

/// Helper function to create JWT tokens (for login endpoint)
pub fn create_jwt_token(
    user_id: String,
    username: String,
    config: &AuthenticationConfig,
) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims::new(user_id, username, config);
    
    let header = Header::default();
    let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_ref());
    
    encode(&header, &claims, &encoding_key)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_claims_creation() {
        let config = create_test_config();
        let claims = Claims::new("user123".to_string(), "Test User".to_string(), &config);
        
        assert_eq!(claims.user_id(), "user123");
        assert_eq!(claims.username, "Test User");
        assert_eq!(claims.iss, "test-issuer");
        assert!(claims.exp > claims.iat); // Expiry after issued time
    }
    
    #[test]
    fn test_jwt_token_creation() {
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