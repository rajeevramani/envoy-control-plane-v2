/// Certificate generator for TLS testing
/// 
/// This utility generates self-signed certificates for local development
/// and testing of the TLS-enabled gRPC server.

use anyhow::Result;
use std::fs;
use std::process::Command;

fn main() -> Result<()> {
    println!("🔐 Generating self-signed certificates for TLS testing...");
    
    let cert_dir = "certs";
    let cert_path = format!("{}/server.crt", cert_dir);
    let key_path = format!("{}/server.key", cert_dir);
    
    // Generate private key
    println!("📝 Generating private key...");
    let key_output = Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm", "RSA",
            "-out", &key_path
        ])
        .output()?;
    
    if !key_output.status.success() {
        anyhow::bail!("Failed to generate private key: {}", String::from_utf8_lossy(&key_output.stderr));
    }
    
    // Generate self-signed certificate
    println!("📜 Generating self-signed certificate...");
    let cert_output = Command::new("openssl")
        .args([
            "req",
            "-new",
            "-x509",
            "-key", &key_path,
            "-out", &cert_path,
            "-days", "365",
            "-subj", "/CN=localhost/O=EnvoyControlPlane/C=US",
            "-addext", "subjectAltName=DNS:localhost,DNS:control-plane,IP:127.0.0.1"
        ])
        .output()?;
    
    if !cert_output.status.success() {
        anyhow::bail!("Failed to generate certificate: {}", String::from_utf8_lossy(&cert_output.stderr));
    }
    
    println!("✅ Certificates generated successfully!");
    println!("📁 Certificate: {}", cert_path);
    println!("🔑 Private key: {}", key_path);
    println!();
    println!("🔒 Certificate is valid for:");
    println!("   - localhost");
    println!("   - control-plane");  
    println!("   - 127.0.0.1");
    println!();
    println!("💡 You can now start the Control Plane with TLS enabled!");
    
    Ok(())
}