/// TLS certificate management for secure gRPC communication
/// 
/// This module handles:
/// - Loading certificates from config.yaml paths
/// - Creating tonic Identity for TLS servers
/// - Configuring secure gRPC servers

use anyhow::{Context, Result};
use std::fs;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use crate::config::TlsConfig;

/// Load TLS identity from configuration
/// 
/// This function reads TLS certificates based on the provided configuration
/// from config.yaml, with fallback to environment variables.
pub fn load_tls_identity(tls_config: &TlsConfig) -> Result<Identity> {
    println!("🔒 Loading TLS certificates...");
    
    // Check if TLS is enabled in config
    if !tls_config.enabled {
        anyhow::bail!("TLS is disabled in configuration");
    }
    
    // Debug: Show environment variable status
    println!("🔍 Checking environment variables:");
    println!("   TLS_CERT_PATH = {:?}", std::env::var("TLS_CERT_PATH"));
    println!("   TLS_KEY_PATH = {:?}", std::env::var("TLS_KEY_PATH"));
    
    // Strategy 1: Use environment variables if set (overrides config)
    if let (Ok(cert_path), Ok(key_path)) = (
        std::env::var("TLS_CERT_PATH"),
        std::env::var("TLS_KEY_PATH")
    ) {
        println!("📂 Using certificates from environment variables");
        println!("   Certificate: {}", cert_path);
        println!("   Private key: {}", key_path);
        return create_identity_from_files(&cert_path, &key_path);
    }
    
    // Strategy 2: Use paths from config.yaml
    println!("📂 Using certificates from configuration");
    println!("   Certificate: {}", tls_config.cert_path);
    println!("   Private key: {}", tls_config.key_path);
    
    create_identity_from_files(&tls_config.cert_path, &tls_config.key_path)
}

/// Create tonic Identity from certificate and key files
/// 
/// Reads PEM-formatted certificate and private key files and creates
/// a tonic Identity that can be used for TLS server configuration.
fn create_identity_from_files(cert_path: &str, key_path: &str) -> Result<Identity> {
    // Verify files exist before trying to read them
    if !std::path::Path::new(cert_path).exists() {
        anyhow::bail!("Certificate file not found: {}", cert_path);
    }
    if !std::path::Path::new(key_path).exists() {
        anyhow::bail!("Private key file not found: {}", key_path);
    }
    
    // Read certificate file (PEM format)
    let cert_pem = fs::read_to_string(cert_path)
        .with_context(|| format!("Failed to read certificate: {}", cert_path))?;
    
    // Read private key file (PEM format)  
    let key_pem = fs::read_to_string(key_path)
        .with_context(|| format!("Failed to read private key: {}", key_path))?;
    
    // Create tonic Identity from PEM data
    let identity = Identity::from_pem(cert_pem, key_pem);
    
    println!("✅ TLS identity created successfully");
    Ok(identity)
}

/// Create a TLS-enabled gRPC server
/// 
/// Takes a tonic Identity and returns a configured Server builder
/// ready for TLS connections with proper gRPC/HTTP2 ALPN configuration.
pub fn create_tls_server(identity: Identity) -> Result<Server> {
    // Create server TLS configuration with explicit HTTP/2 ALPN
    let tls_config = ServerTlsConfig::new()
        .identity(identity);
    
    // Create server with TLS
    let server = Server::builder()
        .tls_config(tls_config)
        .context("Failed to configure TLS for gRPC server")?;
    
    println!("🔐 gRPC server configured with TLS and HTTP/2 ALPN");
    Ok(server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;
    use tempfile::TempDir;

    // Create test certificates in a temporary directory
    fn create_test_certs() -> (TempDir, String, String) {
        let temp_dir = TempDir::new().unwrap();
        
        // Simple test certificate (not real, just for testing)
        let test_cert = "-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAJJ8FJ8FJ8FJMA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNVBAMTCWxv
Y2FsaG9zdDAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBQxEjAQBgNV
BAMTCWxvY2FsaG9zdDBcMA0GCSqGSIb3DQEBAQUAA0sAMEgCQQC1234567890abc
def1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc
def1234567890abcdef1234567890abcdef1234567890abcdef1234567890AgMB
AAEwDQYJKoZIhvcNAQELBQADQQA1234567890abcdef1234567890abcdef123456
7890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12345678
90abcdef1234567890abcdef1234567890
-----END CERTIFICATE-----";

        let test_key = "-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC1234567890abc
def1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc
def1234567890abcdef1234567890abcdef1234567890abcdef1234567890AQAB
AoIBAH1234567890abcdef1234567890abcdef1234567890abcdef1234567890
abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890
-----END PRIVATE KEY-----";

        let cert_path = temp_dir.path().join("test.crt");
        let key_path = temp_dir.path().join("test.key");
        
        fs::write(&cert_path, test_cert).unwrap();
        fs::write(&key_path, test_key).unwrap();
        
        (temp_dir, cert_path.to_string_lossy().to_string(), key_path.to_string_lossy().to_string())
    }

    #[test]
    fn test_load_tls_identity_when_disabled() {
        let tls_config = TlsConfig {
            enabled: false,
            cert_path: "dummy".to_string(),
            key_path: "dummy".to_string(),
        };
        
        let result = load_tls_identity(&tls_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TLS is disabled"));
    }

    #[test]
    fn test_load_tls_identity_from_config() {
        let (_temp_dir, cert_path, key_path) = create_test_certs();
        
        let tls_config = TlsConfig {
            enabled: true,
            cert_path,
            key_path,
        };
        
        let result = load_tls_identity(&tls_config);
        assert!(result.is_ok(), "Should load valid certificates");
    }

    #[test]
    fn test_load_tls_identity_env_override() {
        let (_temp_dir, cert_path, key_path) = create_test_certs();
        
        // Set environment variables
        env::set_var("TLS_CERT_PATH", &cert_path);
        env::set_var("TLS_KEY_PATH", &key_path);
        
        let tls_config = TlsConfig {
            enabled: true,
            cert_path: "wrong_path".to_string(),  // Should be ignored
            key_path: "wrong_path".to_string(),   // Should be ignored
        };
        
        let result = load_tls_identity(&tls_config);
        
        // Clean up environment
        env::remove_var("TLS_CERT_PATH");
        env::remove_var("TLS_KEY_PATH");
        
        assert!(result.is_ok(), "Should use env vars over config paths");
    }

    #[test]
    fn test_load_tls_identity_missing_files() {
        let tls_config = TlsConfig {
            enabled: true,
            cert_path: "/nonexistent/cert.crt".to_string(),
            key_path: "/nonexistent/key.key".to_string(),
        };
        
        let result = load_tls_identity(&tls_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}