use super::{AppConfig, ServerConfig, EnvoyGenerationConfig};
use anyhow::{bail, Result};

/// Configuration validation errors with helpful messages
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Port {port} is invalid: {reason}")]
    InvalidPort { port: u16, reason: String },
    
    #[error("Port conflict: {port1_name} and {port2_name} both use port {port}")]
    PortConflict {
        port1_name: String,
        port2_name: String,
        port: u16,
    },
    
    #[error("Invalid host address '{host}': {reason}")]
    InvalidHost { host: String, reason: String },
    
    #[error("Invalid timeout value {value}: {reason}")]
    InvalidTimeout { value: u64, reason: String },
}

/// Validates the entire application configuration
pub fn validate_config(config: &AppConfig) -> Result<()> {
    validate_server_config(&config.control_plane.server)?;
    validate_envoy_config(&config.envoy_generation)?;
    Ok(())
}

/// Validates server configuration (ports, host)
fn validate_server_config(server: &ServerConfig) -> Result<()> {
    // Validate individual ports
    validate_port(server.rest_port, "rest_port")?;
    validate_port(server.xds_port, "xds_port")?;
    
    // Validate no port conflicts
    validate_no_port_conflicts(server)?;
    
    // Validate host address
    validate_host(&server.host)?;
    
    Ok(())
}

/// Validates a single port is in valid range
fn validate_port(port: u16, port_name: &str) -> Result<()> {
    if port == 0 {
        bail!(ValidationError::InvalidPort {
            port,
            reason: format!("{port_name} cannot be 0 (reserved)")
        });
    }
    
    // Note: u16 max is 65535, so we don't need to check upper bound
    // But we could warn about privileged ports
    if port < 1024 {
        eprintln!("⚠️  Warning: {port_name} {port} is a privileged port (requires root on Unix systems)");
    }
    
    Ok(())
}

/// Validates that no two ports conflict
fn validate_no_port_conflicts(server: &ServerConfig) -> Result<()> {
    if server.rest_port == server.xds_port {
        bail!(ValidationError::PortConflict {
            port1_name: "rest_port".to_string(),
            port2_name: "xds_port".to_string(),
            port: server.rest_port,
        });
    }
    
    Ok(())
}

/// Validates host address format
fn validate_host(host: &str) -> Result<()> {
    if host.is_empty() {
        bail!(ValidationError::InvalidHost {
            host: host.to_string(),
            reason: "host cannot be empty".to_string(),
        });
    }
    
    // Check for obvious invalid characters
    if host.contains(' ') {
        bail!(ValidationError::InvalidHost {
            host: host.to_string(),
            reason: "host cannot contain spaces".to_string(),
        });
    }
    
    // Try to determine if it's an IP address or hostname
    if is_ip_address(host) {
        validate_ip_address(host)?;
    } else {
        validate_hostname(host)?;
    }
    
    Ok(())
}

/// Simple check if string looks like an IP address (contains only digits and dots)
fn is_ip_address(host: &str) -> bool {
    host.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Validates IP address format and ranges
fn validate_ip_address(ip: &str) -> Result<()> {
    let parts: Vec<&str> = ip.split('.').collect();
    
    // Must have exactly 4 parts
    if parts.len() != 4 {
        bail!(ValidationError::InvalidHost {
            host: ip.to_string(),
            reason: "IP address must have 4 octets separated by dots".to_string(),
        });
    }
    
    // Each part must be a valid number 0-255
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            bail!(ValidationError::InvalidHost {
                host: ip.to_string(),
                reason: format!("IP address octet {} is empty", i + 1),
            });
        }
        
        match part.parse::<u16>() {
            Ok(num) if num <= 255 => continue,
            Ok(num) => bail!(ValidationError::InvalidHost {
                host: ip.to_string(),
                reason: format!("IP address octet {} ({}) must be 0-255", i + 1, num),
            }),
            Err(_) => bail!(ValidationError::InvalidHost {
                host: ip.to_string(),
                reason: format!("IP address octet {} ('{}') is not a valid number", i + 1, part),
            }),
        }
    }
    
    Ok(())
}

/// Validates hostname format (basic rules)
fn validate_hostname(hostname: &str) -> Result<()> {
    if hostname.len() > 253 {
        bail!(ValidationError::InvalidHost {
            host: hostname.to_string(),
            reason: "hostname cannot exceed 253 characters".to_string(),
        });
    }
    
    // Check for invalid characters (basic check)
    for c in hostname.chars() {
        if !c.is_ascii_alphanumeric() && c != '.' && c != '-' {
            bail!(ValidationError::InvalidHost {
                host: hostname.to_string(),
                reason: format!("hostname contains invalid character '{}'", c),
            });
        }
    }
    
    // Cannot start or end with dash
    if hostname.starts_with('-') || hostname.ends_with('-') {
        bail!(ValidationError::InvalidHost {
            host: hostname.to_string(),
            reason: "hostname cannot start or end with dash".to_string(),
        });
    }
    
    Ok(())
}

/// Validates Envoy generation configuration
fn validate_envoy_config(envoy: &EnvoyGenerationConfig) -> Result<()> {
    // Validate all ports in Envoy config
    validate_port(envoy.admin.port, "admin.port")?;
    validate_port(envoy.listener.default_port, "listener.default_port")?;
    
    // Validate timeout values
    validate_timeout(envoy.cluster.connect_timeout_seconds, "cluster.connect_timeout_seconds")?;
    
    // Validate admin host
    validate_host(&envoy.admin.host)?;
    validate_host(&envoy.listener.binding_address)?;
    
    Ok(())
}

/// Validates timeout values (must be reasonable for network operations)
fn validate_timeout(timeout_seconds: u64, field_name: &str) -> Result<()> {
    const MIN_TIMEOUT: u64 = 1;   // At least 1 second
    const MAX_TIMEOUT: u64 = 300; // At most 5 minutes
    
    if timeout_seconds == 0 {
        bail!(ValidationError::InvalidTimeout {
            value: timeout_seconds,
            reason: format!("{field_name} cannot be 0 (no timeout doesn't make sense)")
        });
    }
    
    if timeout_seconds < MIN_TIMEOUT {
        bail!(ValidationError::InvalidTimeout {
            value: timeout_seconds,
            reason: format!("{field_name} must be at least {MIN_TIMEOUT} second(s)")
        });
    }
    
    if timeout_seconds > MAX_TIMEOUT {
        bail!(ValidationError::InvalidTimeout {
            value: timeout_seconds,
            reason: format!("{field_name} cannot exceed {MAX_TIMEOUT} seconds (too long)")
        });
    }
    
    // Warn about very short timeouts that might cause issues
    if timeout_seconds < 5 {
        eprintln!("⚠️  Warning: {field_name} {timeout_seconds}s is quite short and may cause connection failures");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ControlPlaneConfig, LoggingConfig, LoadBalancingConfig, EnvoyGenerationConfig};
    use std::path::PathBuf;

    fn create_test_config() -> AppConfig {
        AppConfig {
            control_plane: ControlPlaneConfig {
                server: ServerConfig {
                    rest_port: 8080,
                    xds_port: 18000,
                    host: "0.0.0.0".to_string(),
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                },
                load_balancing: LoadBalancingConfig {
                    envoy_version: "1.24".to_string(),
                    available_policies: vec!["ROUND_ROBIN".to_string()],
                    default_policy: "ROUND_ROBIN".to_string(),
                },
            },
            envoy_generation: EnvoyGenerationConfig {
                // Minimal setup for testing
                config_dir: PathBuf::from("./configs"),
                admin: crate::config::AdminConfig {
                    host: "127.0.0.1".to_string(),
                    port: 9901,
                },
                listener: crate::config::ListenerConfig {
                    binding_address: "0.0.0.0".to_string(),
                    default_port: 10000,
                },
                cluster: crate::config::ClusterConfig {
                    connect_timeout_seconds: 5,
                    discovery_type: "STRICT_DNS".to_string(),
                    dns_lookup_family: "V4_ONLY".to_string(),
                    default_protocol: "TCP".to_string(),
                },
                naming: crate::config::NamingConfig {
                    listener_name: "listener_0".to_string(),
                    virtual_host_name: "local_service".to_string(),
                    route_config_name: "local_route".to_string(),
                    default_domains: vec!["*".to_string()],
                },
                bootstrap: crate::config::BootstrapConfig {
                    node_id: "envoy-test-node".to_string(),
                    node_cluster: "envoy-test-cluster".to_string(),
                    control_plane_host: "control-plane".to_string(),
                    main_listener_name: "main_listener".to_string(),
                    control_plane_cluster_name: "control_plane_cluster".to_string(),
                },
                http_filters: crate::config::HttpFiltersConfig {
                    stat_prefix: "ingress_http".to_string(),
                    router_filter_name: "envoy.filters.http.router".to_string(),
                    hcm_filter_name: "envoy.filters.network.http_connection_manager".to_string(),
                },
            },
        }
    }

    #[test]
    fn test_valid_config() {
        let config = create_test_config();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_invalid_port_zero() {
        let mut config = create_test_config();
        config.control_plane.server.rest_port = 0;
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be 0"));
    }

    #[test]
    fn test_port_conflict() {
        let mut config = create_test_config();
        config.control_plane.server.xds_port = 8080; // Same as rest_port
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Port conflict"));
    }

    #[test]
    fn test_empty_host() {
        let mut config = create_test_config();
        config.control_plane.server.host = "".to_string();
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_host_with_spaces() {
        let mut config = create_test_config();
        config.control_plane.server.host = "my host".to_string();
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("contain spaces"));
    }

    #[test]
    fn test_invalid_ip_address() {
        let mut config = create_test_config();
        config.control_plane.server.host = "192.168.1.300".to_string(); // 300 > 255
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be 0-255"));
    }

    #[test]
    fn test_valid_ip_addresses() {
        let valid_ips = vec!["0.0.0.0", "127.0.0.1", "192.168.1.1", "255.255.255.255"];
        
        for ip in valid_ips {
            let mut config = create_test_config();
            config.control_plane.server.host = ip.to_string();
            
            let result = validate_config(&config);
            assert!(result.is_ok(), "IP {} should be valid", ip);
        }
    }

    #[test]
    fn test_valid_hostnames() {
        let valid_hostnames = vec!["localhost", "example.com", "my-server", "server1"];
        
        for hostname in valid_hostnames {
            let mut config = create_test_config();
            config.control_plane.server.host = hostname.to_string();
            
            let result = validate_config(&config);
            assert!(result.is_ok(), "Hostname {} should be valid", hostname);
        }
    }

    #[test]
    fn test_invalid_hostname() {
        let mut config = create_test_config();
        config.control_plane.server.host = "-invalid".to_string(); // Cannot start with dash
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot start or end with dash"));
    }

    #[test]
    fn test_timeout_zero() {
        let mut config = create_test_config();
        config.envoy_generation.cluster.connect_timeout_seconds = 0;
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be 0"));
    }

    #[test]
    fn test_timeout_too_long() {
        let mut config = create_test_config();
        config.envoy_generation.cluster.connect_timeout_seconds = 400; // > 300 seconds
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 300 seconds"));
    }

    #[test]
    fn test_valid_timeout() {
        let mut config = create_test_config();
        config.envoy_generation.cluster.connect_timeout_seconds = 30; // Valid timeout
        
        let result = validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_admin_port_validation() {
        let mut config = create_test_config();
        config.envoy_generation.admin.port = 0; // Invalid port
        
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("admin.port cannot be 0"));
    }
}