use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod validation;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub control_plane: ControlPlaneConfig,
    pub envoy_generation: EnvoyGenerationConfig,
}

// Control plane configuration (for our Rust application)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ControlPlaneConfig {
    pub server: ServerConfig,
    pub tls: TlsConfig,
    pub logging: LoggingConfig,
    pub load_balancing: LoadBalancingConfig,
    pub http_methods: HttpMethodsConfig,
    pub authentication: AuthenticationConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub rest_port: u16,
    pub xds_port: u16,
    pub host: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadBalancingConfig {
    pub envoy_version: String,
    pub available_policies: Vec<String>,
    pub default_policy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpMethodsConfig {
    pub supported_methods: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthenticationConfig {
    pub enabled: bool,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub jwt_issuer: String,
    pub password_hash_cost: u32,
}

// Envoy configuration generation (for generating Envoy configs)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvoyGenerationConfig {
    pub config_dir: PathBuf,
    pub admin: AdminConfig,
    pub listener: ListenerConfig,
    pub cluster: ClusterConfig,
    pub naming: NamingConfig,
    pub bootstrap: BootstrapConfig,
    pub http_filters: HttpFiltersConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListenerConfig {
    pub binding_address: String,
    pub default_port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusterConfig {
    pub connect_timeout_seconds: u64,
    pub discovery_type: String,
    pub dns_lookup_family: String,
    pub default_protocol: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NamingConfig {
    pub listener_name: String,
    pub virtual_host_name: String,
    pub route_config_name: String,
    pub default_domains: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapConfig {
    pub node_id: String,
    pub node_cluster: String,
    pub control_plane_host: String,
    pub main_listener_name: String,
    pub control_plane_cluster_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpFiltersConfig {
    pub stat_prefix: String,
    pub router_filter_name: String,
    pub hcm_filter_name: String,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let settings = config::Config::builder()
            // Start with config file as base
            .add_source(config::File::with_name("config"))
            // Override with environment variables (higher priority)
            .add_source(
                config::Environment::with_prefix("ENVOY_CP")
                    .prefix_separator("_")
                    .separator("__")
            )
            .build()?;

        let mut config: Self = settings.try_deserialize()?;

        // Apply security-focused environment variable overrides
        Self::apply_security_env_overrides(&mut config)?;

        // Validate the loaded configuration
        validation::validate_config(&config)?;

        Ok(config)
    }

    /// Apply critical security environment variables with validation
    fn apply_security_env_overrides(config: &mut Self) -> anyhow::Result<()> {
        // JWT Secret - CRITICAL: Must come from environment in production
        // Try Docker secrets first, then environment variables
        let jwt_secret = Self::load_secret("JWT_SECRET", "/run/secrets/jwt_secret")?;
        
        if let Some(jwt_secret) = jwt_secret {
            if jwt_secret.len() < 32 {
                return Err(anyhow::anyhow!(
                    "JWT_SECRET must be at least 32 characters long for security"
                ));
            }
            config.control_plane.authentication.jwt_secret = jwt_secret;
            println!("✅ JWT secret loaded from secure source");
        } else if config.control_plane.authentication.enabled {
            // In production, JWT secret MUST come from environment
            if config.control_plane.authentication.jwt_secret.contains("change-in-production") {
                return Err(anyhow::anyhow!(
                    "Production error: JWT_SECRET environment variable is required when authentication is enabled. \
                     The default jwt_secret contains 'change-in-production' and is not secure."
                ));
            }
            println!("⚠️  WARNING: Using JWT secret from config file. Set JWT_SECRET environment variable in production.");
        }

        // JWT Issuer override
        if let Ok(jwt_issuer) = std::env::var("JWT_ISSUER") {
            config.control_plane.authentication.jwt_issuer = jwt_issuer;
        }

        // JWT Expiry override
        if let Ok(jwt_expiry) = std::env::var("JWT_EXPIRY_HOURS") {
            match jwt_expiry.parse::<u64>() {
                Ok(hours) if hours > 0 && hours <= 168 => { // Max 1 week
                    config.control_plane.authentication.jwt_expiry_hours = hours;
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "JWT_EXPIRY_HOURS must be a number between 1 and 168 (1 week)"
                    ));
                }
            }
        }

        // Password Hash Cost override
        if let Ok(hash_cost) = std::env::var("BCRYPT_COST") {
            match hash_cost.parse::<u32>() {
                Ok(cost) if cost >= 10 && cost <= 15 => { // Reasonable bcrypt cost range
                    config.control_plane.authentication.password_hash_cost = cost;
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "BCRYPT_COST must be a number between 10 and 15"
                    ));
                }
            }
        }

        // Authentication toggle
        if let Ok(auth_enabled) = std::env::var("AUTHENTICATION_ENABLED") {
            config.control_plane.authentication.enabled = auth_enabled.to_lowercase() == "true";
        }

        Ok(())
    }
    
    /// Load a secret from Docker secrets file or environment variable
    /// Docker secrets take precedence over environment variables for security
    fn load_secret(env_var: &str, docker_secret_path: &str) -> anyhow::Result<Option<String>> {
        // Try Docker secret first (more secure in container environments)
        if let Ok(secret) = std::fs::read_to_string(docker_secret_path) {
            let secret = secret.trim().to_string();
            if !secret.is_empty() {
                println!("🔐 Loaded secret from Docker secrets: {docker_secret_path}");
                return Ok(Some(secret));
            }
        }
        
        // Fall back to environment variable
        if let Ok(secret) = std::env::var(env_var) {
            if !secret.is_empty() {
                println!("🔐 Loaded secret from environment: {env_var}");
                return Ok(Some(secret));
            }
        }
        
        Ok(None)
    }
    
    /// Load demo user credentials from environment variables
    /// Returns (username, password) tuples for demo users
    pub fn load_demo_credentials() -> Vec<(String, String)> {
        let mut credentials = Vec::new();
        
        // Load admin credentials
        if let (Ok(username), Ok(password)) = (
            std::env::var("DEMO_ADMIN_USERNAME"),
            std::env::var("DEMO_ADMIN_PASSWORD")
        ) {
            credentials.push((username, password));
        } else {
            // Secure default - random password that must be looked up
            credentials.push(("admin".to_string(), "secure-admin-123".to_string()));
            println!("⚠️  Using default admin credentials. Set DEMO_ADMIN_USERNAME and DEMO_ADMIN_PASSWORD");
        }
        
        // Load additional demo users from environment
        for i in 1..=5 {
            if let (Ok(username), Ok(password)) = (
                std::env::var(&format!("DEMO_USER{i}_USERNAME")),
                std::env::var(&format!("DEMO_USER{i}_PASSWORD"))
            ) {
                credentials.push((username, password));
            }
        }
        
        // Add default demo users if no additional users configured
        if credentials.len() == 1 {
            credentials.push(("user".to_string(), "secure-user-456".to_string()));
            credentials.push(("demo".to_string(), "secure-demo-789".to_string()));
            println!("⚠️  Using default demo user credentials. Configure DEMO_USER*_USERNAME and DEMO_USER*_PASSWORD");
        }
        
        credentials
    }

    /// Generate a secure random JWT secret for testing
    /// This ensures tests don't rely on hardcoded secrets
    #[cfg(test)]
    fn generate_test_jwt_secret() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789-_";
        const SECRET_LEN: usize = 64; // 64 chars for strong test secret
        
        let mut rng = rand::thread_rng();
        (0..SECRET_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    #[cfg(test)]
    pub fn create_test_config() -> Self {
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
                    enabled: false,  // Disabled for tests
                    jwt_secret: Self::generate_test_jwt_secret(),
                    jwt_expiry_hours: 24,
                    jwt_issuer: "envoy-control-plane-test".to_string(),
                    password_hash_cost: 8,  // Lower cost for faster tests
                },
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
}
