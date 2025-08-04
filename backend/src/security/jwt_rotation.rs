use std::sync::Arc;
use tokio::{sync::RwLock, time::{Duration, interval}};
use anyhow::Result;
use tracing::{info, warn, error};

use crate::config::AppConfig;
use crate::auth::JwtKeys;

/// JWT Secret Rotation Manager
/// Handles periodic rotation of JWT secrets for enhanced security
pub struct JwtRotationManager {
    current_keys: Arc<RwLock<JwtKeys>>,
    rotation_interval: Duration,
    config: AppConfig,
}

impl JwtRotationManager {
    pub fn new(keys: JwtKeys, config: AppConfig, rotation_hours: u64) -> Self {
        Self {
            current_keys: Arc::new(RwLock::new(keys)),
            rotation_interval: Duration::from_secs(rotation_hours * 3600),
            config,
        }
    }
    
    /// Get current JWT keys (thread-safe)
    pub async fn get_keys(&self) -> JwtKeys {
        self.current_keys.read().await.clone()
    }
    
    /// Start the rotation background task
    pub async fn start_rotation_task(self: Arc<Self>) {
        let mut ticker = interval(self.rotation_interval);
        
        info!("🔄 JWT rotation manager started (interval: {:?})", self.rotation_interval);
        
        loop {
            ticker.tick().await;
            
            if let Err(e) = self.rotate_secret().await {
                error!("❌ JWT secret rotation failed: {}", e);
            }
        }
    }
    
    /// Rotate the JWT secret
    async fn rotate_secret(&self) -> Result<()> {
        info!("🔄 Starting JWT secret rotation...");
        
        // Load new secret from secure source
        let new_secret = Self::load_rotation_secret()
            .or_else(|| Self::generate_secure_secret())
            .ok_or_else(|| anyhow::anyhow!("Failed to obtain new JWT secret"))?;
        
        // Create new JWT keys with the rotated secret
        let mut auth_config = self.config.control_plane.authentication.clone();
        auth_config.jwt_secret = new_secret;
        let new_keys = JwtKeys::new(auth_config);
        
        // Update the current keys atomically
        {
            let mut current_keys = self.current_keys.write().await;
            *current_keys = new_keys;
        }
        
        info!("✅ JWT secret rotation completed successfully");
        Ok(())
    }
    
    /// Load rotation secret from secure source
    /// This should be implemented based on your secret management system
    fn load_rotation_secret() -> Option<String> {
        // Try to load from rotation-specific environment variable
        if let Ok(secret) = std::env::var("JWT_SECRET_ROTATION") {
            if secret.len() >= 32 {
                info!("🔐 Loaded rotation secret from JWT_SECRET_ROTATION");
                return Some(secret);
            }
        }
        
        // Try to load from Docker secrets rotation file
        if let Ok(secret) = std::fs::read_to_string("/run/secrets/jwt_secret_rotation") {
            let secret = secret.trim();
            if secret.len() >= 32 {
                info!("🔐 Loaded rotation secret from Docker secrets");
                return Some(secret.to_string());
            }
        }
        
        None
    }
    
    /// Generate a cryptographically secure secret for rotation
    fn generate_secure_secret() -> Option<String> {
        use rand::Rng;
        
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789-_!@#$%^&*";
        const SECRET_LEN: usize = 128; // Very strong secret for production
        
        let mut rng = rand::thread_rng();
        let secret: String = (0..SECRET_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        
        warn!("⚠️  Generated secure random JWT secret for rotation (not recommended for production)");
        Some(secret)
    }
}

/// JWT Rotation Configuration
#[derive(Debug, Clone)]
pub struct RotationConfig {
    pub enabled: bool,
    pub interval_hours: u64,
    pub overlap_duration_minutes: u64, // How long to accept both old and new tokens
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for safety
            interval_hours: 168, // Weekly rotation
            overlap_duration_minutes: 60, // 1 hour overlap
        }
    }
}

impl RotationConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("JWT_ROTATION_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
        
        let interval_hours = std::env::var("JWT_ROTATION_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(168); // Default: weekly
        
        let overlap_duration_minutes = std::env::var("JWT_ROTATION_OVERLAP_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60); // Default: 1 hour
        
        Self {
            enabled,
            interval_hours,
            overlap_duration_minutes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_config() -> AppConfig {
        crate::config::AppConfig::create_test_config()
    }
    
    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.interval_hours, 168);
        assert_eq!(config.overlap_duration_minutes, 60);
    }
    
    #[test]
    fn test_secure_secret_generation() {
        let secret = JwtRotationManager::generate_secure_secret();
        assert!(secret.is_some());
        let secret = secret.unwrap();
        assert!(secret.len() >= 32);
        assert!(secret.chars().all(|c| c.is_ascii()));
    }
    
    #[tokio::test]
    async fn test_rotation_manager_creation() {
        let config = create_test_config();
        let jwt_keys = JwtKeys::new(config.control_plane.authentication.clone());
        let manager = JwtRotationManager::new(jwt_keys, config, 24);
        
        let keys = manager.get_keys().await;
        assert!(!keys.config.jwt_secret.is_empty());
    }
}