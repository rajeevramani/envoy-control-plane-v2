use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Load balancing policy for clusters
/// Hybrid approach: known policies as variants + Custom for flexibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoadBalancingPolicy {
    RoundRobin,
    LeastRequest,
    Random,
    RingHash,
    Custom(String), // For new/unknown policies
}

impl FromStr for LoadBalancingPolicy {
    type Err = LoadBalancingPolicyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Validate input
        if s.is_empty() {
            return Err(LoadBalancingPolicyParseError::Empty);
        }
        
        // Check for invalid characters that might break configuration
        if s.contains('\n') || s.contains('\r') || s.contains('\0') {
            return Err(LoadBalancingPolicyParseError::InvalidCharacters(s.to_string()));
        }
        
        let policy = match s {
            "ROUND_ROBIN" => LoadBalancingPolicy::RoundRobin,
            "LEAST_REQUEST" => LoadBalancingPolicy::LeastRequest,
            "RANDOM" => LoadBalancingPolicy::Random,
            "RING_HASH" => LoadBalancingPolicy::RingHash,
            custom => LoadBalancingPolicy::Custom(custom.to_string()),
        };
        Ok(policy)
    }
}

/// Errors that can occur when parsing LoadBalancingPolicy
#[derive(Debug, thiserror::Error)]
pub enum LoadBalancingPolicyParseError {
    #[error("Load balancing policy cannot be empty")]
    Empty,
    
    #[error("Load balancing policy contains invalid characters: '{0}'")]
    InvalidCharacters(String),
}

impl LoadBalancingPolicy {
    /// Safe parsing method that provides better error context
    pub fn parse_safe(s: &str, context: &str) -> Result<Self, String> {
        s.parse().map_err(|e: LoadBalancingPolicyParseError| {
            format!("Failed to parse load balancing policy in {context}: {e}")
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub name: String,         // Primary identifier aligned with Envoy conventions
    pub path: String,
    pub cluster_name: String,
    pub prefix_rewrite: Option<String>,
    pub http_methods: Option<Vec<String>>, // GET, POST, PUT, DELETE, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub name: String,
    pub endpoints: Vec<Endpoint>,
    pub lb_policy: Option<LoadBalancingPolicy>, // Optional: falls back to config default
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Route {
    pub fn new(name: String, path: String, cluster_name: String, prefix_rewrite: Option<String>) -> Self {
        Self {
            name,
            path,
            cluster_name,
            prefix_rewrite,
            http_methods: None,
        }
    }

    pub fn with_methods(
        name: String,
        path: String,
        cluster_name: String,
        prefix_rewrite: Option<String>,
        http_methods: Option<Vec<String>>,
    ) -> Self {
        Self {
            name,
            path,
            cluster_name,
            prefix_rewrite,
            http_methods,
        }
    }
}

impl Cluster {
    pub fn new(name: String, endpoints: Vec<Endpoint>) -> Self {
        Self {
            name,
            endpoints,
            lb_policy: None, // No specific policy - will use system default
        }
    }

    pub fn with_lb_policy(
        name: String,
        endpoints: Vec<Endpoint>,
        lb_policy: LoadBalancingPolicy,
    ) -> Self {
        Self {
            name,
            endpoints,
            lb_policy: Some(lb_policy),
        }
    }
}

impl Endpoint {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}

/// HTTP Filter - Simple approach with JSON config for MVP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpFilter {
    pub name: String,
    pub filter_type: String, // "rate_limit", "cors", etc.
    pub config: serde_json::Value, // Flexible JSON config
    pub enabled: bool,
}

/// Route-Filter association
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteFilters {
    pub route_name: String,
    pub filter_names: Vec<String>, // References to HttpFilter names
    pub custom_order: Option<Vec<String>>, // Override global order
}

impl HttpFilter {
    pub fn new(name: String, filter_type: String, config: serde_json::Value) -> Self {
        Self {
            name,
            filter_type,
            config,
            enabled: true,
        }
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Basic validation for the filter
    pub fn validate(&self, supported_filters: &[String]) -> Result<(), String> {
        // Validate name
        if self.name.is_empty() {
            return Err("Filter name cannot be empty".to_string());
        }

        // Validate name format (alphanumeric, underscore, hyphen)
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err("Filter name can only contain alphanumeric characters, underscores, and hyphens".to_string());
        }

        // Validate filter_type
        if self.filter_type.is_empty() {
            return Err("Filter type cannot be empty".to_string());
        }

        if !supported_filters.contains(&self.filter_type) {
            return Err(format!("Unsupported filter type: {}", self.filter_type));
        }

        // Basic config validation
        if self.config.is_null() {
            return Err("Filter config cannot be null".to_string());
        }

        Ok(())
    }
}

impl RouteFilters {
    pub fn new(route_name: String, filter_names: Vec<String>) -> Self {
        Self {
            route_name,
            filter_names,
            custom_order: None,
        }
    }

    pub fn with_custom_order(mut self, order: Vec<String>) -> Self {
        self.custom_order = Some(order);
        self
    }

    /// Validate that all referenced filters exist
    pub fn validate(&self, existing_filters: &[String]) -> Result<(), String> {
        if self.route_name.is_empty() {
            return Err("Route name cannot be empty".to_string());
        }

        for filter_name in &self.filter_names {
            if !existing_filters.contains(filter_name) {
                return Err(format!("Referenced filter '{}' does not exist", filter_name));
            }
        }

        // Validate custom order if provided
        if let Some(ref custom_order) = self.custom_order {
            for filter_name in custom_order {
                if !self.filter_names.contains(filter_name) {
                    return Err(format!("Custom order references filter '{}' not in filter list", filter_name));
                }
            }
        }

        Ok(())
    }
}

// Helper functions for creating common HTTP filters
impl HttpFilter {
    /// Create a rate limit filter with basic configuration
    pub fn create_rate_limit_filter(
        name: String,
        requests_per_unit: u32,
        unit: &str, // "second", "minute", "hour", "day"
        burst_size: Option<u32>,
    ) -> Result<Self, String> {
        let unit = match unit.to_lowercase().as_str() {
            "second" | "seconds" => "second",
            "minute" | "minutes" => "minute",
            "hour" | "hours" => "hour",
            "day" | "days" => "day",
            _ => return Err("Invalid time unit. Use: second, minute, hour, or day".to_string()),
        };

        if requests_per_unit == 0 {
            return Err("requests_per_unit must be greater than 0".to_string());
        }

        let config = serde_json::json!({
            "requests_per_unit": requests_per_unit,
            "unit": unit,
            "burst_size": burst_size,
            "key": "client_ip" // Default to client IP-based rate limiting
        });

        Ok(HttpFilter::new(name, "rate_limit".to_string(), config))
    }

    /// Create a CORS filter with basic configuration
    pub fn create_cors_filter(
        name: String,
        allowed_origins: Vec<String>,
        allowed_methods: Vec<String>,
        allowed_headers: Vec<String>,
        allow_credentials: bool,
    ) -> Result<Self, String> {
        if allowed_origins.is_empty() {
            return Err("allowed_origins cannot be empty".to_string());
        }

        if allowed_methods.is_empty() {
            return Err("allowed_methods cannot be empty".to_string());
        }

        // Validate HTTP methods
        let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
        for method in &allowed_methods {
            if !valid_methods.contains(&method.to_uppercase().as_str()) {
                return Err(format!("Invalid HTTP method: {}", method));
            }
        }

        let config = serde_json::json!({
            "allowed_origins": allowed_origins,
            "allowed_methods": allowed_methods,
            "allowed_headers": allowed_headers,
            "allow_credentials": allow_credentials,
            "max_age": 86400 // 24 hours default
        });

        Ok(HttpFilter::new(name, "cors".to_string(), config))
    }

    /// Validate rate_limit filter configuration
    pub fn validate_rate_limit_config(config: &serde_json::Value) -> Result<(), String> {
        let requests_per_unit = config.get("requests_per_unit")
            .and_then(|v| v.as_u64())
            .ok_or("Missing or invalid 'requests_per_unit'")?;

        if requests_per_unit == 0 {
            return Err("requests_per_unit must be greater than 0".to_string());
        }

        if requests_per_unit > 100_000 {
            return Err("requests_per_unit too high (max 100,000)".to_string());
        }

        let unit = config.get("unit")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'unit'")?;

        match unit {
            "second" | "minute" | "hour" | "day" => {},
            _ => return Err("Invalid unit. Use: second, minute, hour, or day".to_string()),
        }

        if let Some(burst_size) = config.get("burst_size").and_then(|v| v.as_u64()) {
            if burst_size > requests_per_unit * 10 {
                return Err("burst_size too large (max 10x requests_per_unit)".to_string());
            }
        }

        Ok(())
    }

    /// Validate CORS filter configuration
    pub fn validate_cors_config(config: &serde_json::Value) -> Result<(), String> {
        let allowed_origins = config.get("allowed_origins")
            .and_then(|v| v.as_array())
            .ok_or("Missing 'allowed_origins' array")?;

        if allowed_origins.is_empty() {
            return Err("allowed_origins cannot be empty".to_string());
        }

        let allowed_methods = config.get("allowed_methods")
            .and_then(|v| v.as_array())
            .ok_or("Missing 'allowed_methods' array")?;

        if allowed_methods.is_empty() {
            return Err("allowed_methods cannot be empty".to_string());
        }

        // Validate HTTP methods
        let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
        for method in allowed_methods {
            if let Some(method_str) = method.as_str() {
                if !valid_methods.contains(&method_str.to_uppercase().as_str()) {
                    return Err(format!("Invalid HTTP method: {}", method_str));
                }
            } else {
                return Err("HTTP methods must be strings".to_string());
            }
        }

        Ok(())
    }

    /// Create a header manipulation filter with basic configuration
    pub fn create_header_manipulation_filter(
        name: String,
        request_headers_to_add: Vec<(String, String)>,
        request_headers_to_remove: Vec<String>,
        response_headers_to_add: Vec<(String, String)>,
        response_headers_to_remove: Vec<String>,
    ) -> Result<Self, String> {
        if request_headers_to_add.is_empty() && request_headers_to_remove.is_empty() && 
           response_headers_to_add.is_empty() && response_headers_to_remove.is_empty() {
            return Err("header_manipulation filter must have at least one operation".to_string());
        }

        // Validate header names
        for (header_name, _) in &request_headers_to_add {
            if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                return Err(format!("Invalid header name: '{}'", header_name));
            }
        }

        for header_name in &request_headers_to_remove {
            if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                return Err(format!("Invalid header name: '{}'", header_name));
            }
        }

        for (header_name, _) in &response_headers_to_add {
            if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                return Err(format!("Invalid header name: '{}'", header_name));
            }
        }

        for header_name in &response_headers_to_remove {
            if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                return Err(format!("Invalid header name: '{}'", header_name));
            }
        }

        let config = serde_json::json!({
            "request_headers_to_add": request_headers_to_add.into_iter()
                .map(|(name, value)| serde_json::json!({"header": {"key": name, "value": value}}))
                .collect::<Vec<_>>(),
            "request_headers_to_remove": request_headers_to_remove,
            "response_headers_to_add": response_headers_to_add.into_iter()
                .map(|(name, value)| serde_json::json!({"header": {"key": name, "value": value}}))
                .collect::<Vec<_>>(),
            "response_headers_to_remove": response_headers_to_remove
        });

        Ok(HttpFilter::new(name, "header_manipulation".to_string(), config))
    }

    /// Validate header_manipulation filter configuration
    pub fn validate_header_manipulation_config(config: &serde_json::Value) -> Result<(), String> {
        if !config.is_object() {
            return Err("header_manipulation config must be an object".to_string());
        }

        // Check that at least one operation is defined
        let has_request_add = config.get("request_headers_to_add")
            .map(|v| v.as_array().map_or(false, |arr| !arr.is_empty()))
            .unwrap_or(false);
        let has_request_remove = config.get("request_headers_to_remove")
            .map(|v| v.as_array().map_or(false, |arr| !arr.is_empty()))
            .unwrap_or(false);
        let has_response_add = config.get("response_headers_to_add")
            .map(|v| v.as_array().map_or(false, |arr| !arr.is_empty()))
            .unwrap_or(false);
        let has_response_remove = config.get("response_headers_to_remove")
            .map(|v| v.as_array().map_or(false, |arr| !arr.is_empty()))
            .unwrap_or(false);

        if !has_request_add && !has_request_remove && !has_response_add && !has_response_remove {
            return Err("header_manipulation must have at least one operation".to_string());
        }

        // Validate request headers to add
        if let Some(headers) = config.get("request_headers_to_add").and_then(|v| v.as_array()) {
            for header in headers {
                if let Some(header_obj) = header.get("header") {
                    if let Some(key) = header_obj.get("key").and_then(|k| k.as_str()) {
                        if key.is_empty() || key.contains(' ') || key.contains(':') {
                            return Err(format!("Invalid header name: '{}'", key));
                        }
                    } else {
                        return Err("Missing header key in request_headers_to_add".to_string());
                    }
                } else {
                    return Err("Invalid header format in request_headers_to_add".to_string());
                }
            }
        }

        // Validate request headers to remove
        if let Some(headers) = config.get("request_headers_to_remove").and_then(|v| v.as_array()) {
            for header in headers {
                if let Some(header_name) = header.as_str() {
                    if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                        return Err(format!("Invalid header name: '{}'", header_name));
                    }
                } else {
                    return Err("request_headers_to_remove must contain strings".to_string());
                }
            }
        }

        // Similar validation for response headers
        if let Some(headers) = config.get("response_headers_to_add").and_then(|v| v.as_array()) {
            for header in headers {
                if let Some(header_obj) = header.get("header") {
                    if let Some(key) = header_obj.get("key").and_then(|k| k.as_str()) {
                        if key.is_empty() || key.contains(' ') || key.contains(':') {
                            return Err(format!("Invalid header name: '{}'", key));
                        }
                    } else {
                        return Err("Missing header key in response_headers_to_add".to_string());
                    }
                } else {
                    return Err("Invalid header format in response_headers_to_add".to_string());
                }
            }
        }

        if let Some(headers) = config.get("response_headers_to_remove").and_then(|v| v.as_array()) {
            for header in headers {
                if let Some(header_name) = header.as_str() {
                    if header_name.is_empty() || header_name.contains(' ') || header_name.contains(':') {
                        return Err(format!("Invalid header name: '{}'", header_name));
                    }
                } else {
                    return Err("response_headers_to_remove must contain strings".to_string());
                }
            }
        }

        Ok(())
    }

    /// Enhanced validate method that includes type-specific validation
    pub fn validate_with_type_check(&self, supported_filters: &[String]) -> Result<(), String> {
        // Basic validation first
        self.validate(supported_filters)?;

        // Type-specific validation
        match self.filter_type.as_str() {
            "rate_limit" => Self::validate_rate_limit_config(&self.config)?,
            "cors" => Self::validate_cors_config(&self.config)?,
            "header_manipulation" => Self::validate_header_manipulation_config(&self.config)?,
            "authentication" => {
                // Basic structure check for authentication
                if !self.config.is_object() {
                    return Err("authentication config must be an object".to_string());
                }
            },
            "request_validation" => {
                // Basic structure check for request validation
                if !self.config.is_object() {
                    return Err("request_validation config must be an object".to_string());
                }
            },
            _ => {
                // Unknown filter type should have been caught by basic validation
                return Err(format!("Unknown filter type: {}", self.filter_type));
            }
        }

        Ok(())
    }
}
