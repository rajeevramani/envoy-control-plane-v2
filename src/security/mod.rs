/// Security module for TLS configuration
/// 
/// This module provides TLS certificate management and server configuration
/// for secure gRPC communication between Envoy and the Control Plane.

pub mod tls;

// Re-export TLS functions for easy use
pub use tls::*;