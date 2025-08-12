use crate::xds::conversion::ConversionError;
use std::fmt;

/// Common validation utilities for security and data integrity
/// Consolidates all validation logic to reduce duplication

/// Unified validation error type
#[derive(Debug, Clone)]
pub enum ValidationError {
    Empty { field_name: String },
    TooLong { field_name: String, max_length: usize, actual_length: usize },
    TooShort { field_name: String, min_length: usize, actual_length: usize },
    InvalidCharacters { field_name: String, allowed_chars: String },
    InvalidFormat { field_name: String, expected_format: String },
    SecurityViolation { field_name: String, reason: String },
    InvalidValue { field_name: String, reason: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::Empty { field_name } => {
                write!(f, "Field '{}' cannot be empty", field_name)
            }
            ValidationError::TooLong { field_name, max_length, actual_length } => {
                write!(f, "Field '{}' exceeds maximum length of {} characters (actual: {})", 
                       field_name, max_length, actual_length)
            }
            ValidationError::TooShort { field_name, min_length, actual_length } => {
                write!(f, "Field '{}' must be at least {} characters long (actual: {})", 
                       field_name, min_length, actual_length)
            }
            ValidationError::InvalidCharacters { field_name, allowed_chars } => {
                write!(f, "Field '{}' contains invalid characters. Allowed: {}", 
                       field_name, allowed_chars)
            }
            ValidationError::InvalidFormat { field_name, expected_format } => {
                write!(f, "Field '{}' has invalid format. Expected: {}", 
                       field_name, expected_format)
            }
            ValidationError::SecurityViolation { field_name, reason } => {
                write!(f, "Security violation in field '{}': {}", field_name, reason)
            }
            ValidationError::InvalidValue { field_name, reason } => {
                write!(f, "Invalid value for field '{}': {}", field_name, reason)
            }
        }
    }
}

impl From<ValidationError> for ConversionError {
    fn from(err: ValidationError) -> Self {
        ConversionError::ValidationFailed { 
            reason: err.to_string() 
        }
    }
}

/// Common validation functions
pub struct Validator;

impl Validator {
    /// Validate string length with unified error handling
    pub fn validate_length(
        value: &str, 
        field_name: &str, 
        min: Option<usize>, 
        max: Option<usize>
    ) -> Result<(), ValidationError> {
        let len = value.len();
        
        // Only reject empty if a minimum length is explicitly required
        if len == 0 && min.is_some() && min.unwrap() > 0 {
            return Err(ValidationError::Empty { 
                field_name: field_name.to_string() 
            });
        }
        
        if let Some(min_len) = min {
            if len < min_len {
                return Err(ValidationError::TooShort { 
                    field_name: field_name.to_string(),
                    min_length: min_len,
                    actual_length: len,
                });
            }
        }
        
        if let Some(max_len) = max {
            if len > max_len {
                return Err(ValidationError::TooLong { 
                    field_name: field_name.to_string(),
                    max_length: max_len,
                    actual_length: len,
                });
            }
        }
        
        Ok(())
    }
    
    /// Validate character set (alphanumeric + specific allowed chars)
    pub fn validate_charset(
        value: &str, 
        field_name: &str, 
        additional_allowed: &[char]
    ) -> Result<(), ValidationError> {
        let is_valid = value.chars().all(|c| {
            c.is_ascii_alphanumeric() || additional_allowed.contains(&c)
        });
        
        if !is_valid {
            let mut allowed = String::from("alphanumeric");
            if !additional_allowed.is_empty() {
                allowed.push_str(" and: ");
                for (i, ch) in additional_allowed.iter().enumerate() {
                    if i > 0 { allowed.push_str(", "); }
                    allowed.push(*ch);
                }
            }
            
            return Err(ValidationError::InvalidCharacters {
                field_name: field_name.to_string(),
                allowed_chars: allowed,
            });
        }
        
        Ok(())
    }

    /// Detect Lua injection patterns (consolidated from conversion.rs)
    pub fn validate_lua_safety(input: &str, field_name: &str) -> Result<(), ValidationError> {
        let dangerous_patterns = [
            // Lua execution functions
            "os.execute", "io.popen", "loadstring", "load(", "dofile", "loadfile",
            
            // Debug and system access
            "debug.", "package.", "require(", "_g[", "getfenv", "setfenv",
            
            // String manipulation that could break out of context
            "]] ..", ".. [[", "\nend\n", "\nfunction", "\nlocal", "\nif",
            "\nfor", "\nwhile", "\nrepeat", "\ndo\n",
            
            // Comment injection attempts
            "--]]", "]]--", "/*", "*/",
            
            // Script termination attempts  
            "\0", "\\0", "\nreturn", "return\n",
        ];

        let input_lower = input.to_lowercase();
        for pattern in &dangerous_patterns {
            if input_lower.contains(pattern) {
                return Err(ValidationError::SecurityViolation {
                    field_name: field_name.to_string(),
                    reason: format!("Contains potentially dangerous Lua pattern '{}'", pattern),
                });
            }
        }

        // Check for excessive control characters
        let control_char_count = input.chars().filter(|c| c.is_control()).count();
        if control_char_count > 2 {
            return Err(ValidationError::SecurityViolation {
                field_name: field_name.to_string(),
                reason: format!("Contains excessive control characters ({})", control_char_count),
            });
        }

        Ok(())
    }

    /// Consolidated HTTP header name validation  
    pub fn validate_http_header_name(name: &str) -> Result<(), ValidationError> {
        Self::validate_length(name, "header_name", Some(1), Some(100))?;
        Self::validate_lua_safety(name, "header_name")?;
        Self::validate_charset(name, "header_name", &['-', '_', '.'])?;
        Ok(())
    }

    /// Consolidated HTTP header value validation
    pub fn validate_http_header_value(value: &str) -> Result<(), ValidationError> {
        // Empty header values are valid in HTTP
        if value.is_empty() {
            return Ok(());
        }
        
        // For non-empty values, validate length (no minimum since empty is already handled)
        Self::validate_length(value, "header_value", None, Some(8192))?;
        Self::validate_lua_safety(value, "header_value")?;
        
        // HTTP header values can contain most characters but reject most control chars
        // Tab is allowed in HTTP header values (RFC 7230)
        if value.chars().any(|c| c.is_control() && c != '\t') {
            return Err(ValidationError::InvalidCharacters {
                field_name: "header_value".to_string(),
                allowed_chars: "any printable characters (control chars except tab not allowed)".to_string(),
            });
        }
        
        Ok(())
    }

    /// JWT secret validation (consolidated from conversion.rs)
    pub fn validate_jwt_secret(secret: &str) -> Result<(), ValidationError> {
        Self::validate_length(secret, "jwt_secret", Some(32), None)?;
        
        // Check for weak patterns
        let secret_lower = secret.to_lowercase();
        let weak_patterns = ["secret", "password"];
        
        for pattern in &weak_patterns {
            if secret_lower.contains(pattern) {
                return Err(ValidationError::SecurityViolation {
                    field_name: "jwt_secret".to_string(),
                    reason: format!("Contains weak pattern '{}'", pattern),
                });
            }
        }
        
        // Check for repeated characters (weak entropy)
        if secret == "a".repeat(secret.len()) || secret == "1".repeat(secret.len()) {
            return Err(ValidationError::SecurityViolation {
                field_name: "jwt_secret".to_string(),
                reason: "Appears to be weak (repeated characters)".to_string(),
            });
        }
        
        Ok(())
    }

    /// Resource name validation (routes, clusters, etc.)
    pub fn validate_resource_name(name: &str, resource_type: &str, max_length: usize) -> Result<(), ValidationError> {
        Self::validate_length(name, &format!("{}_name", resource_type), Some(1), Some(max_length))?;
        Self::validate_charset(name, &format!("{}_name", resource_type), &['-', '_', '.'])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_validation() {
        // Empty
        assert!(Validator::validate_length("", "test", Some(1), None).is_err());
        
        // Too short
        assert!(Validator::validate_length("ab", "test", Some(3), None).is_err());
        
        // Too long
        assert!(Validator::validate_length("abcde", "test", None, Some(3)).is_err());
        
        // Valid
        assert!(Validator::validate_length("abc", "test", Some(2), Some(5)).is_ok());
    }

    #[test]
    fn test_lua_safety_validation() {
        // Safe input
        assert!(Validator::validate_lua_safety("safe_input", "test").is_ok());
        
        // Dangerous patterns
        assert!(Validator::validate_lua_safety("os.execute('rm -rf /')", "test").is_err());
        assert!(Validator::validate_lua_safety("]] .. evil", "test").is_err());
        assert!(Validator::validate_lua_safety("\nend\nfunction", "test").is_err());
    }

    #[test] 
    fn test_header_validation() {
        // Valid header name
        assert!(Validator::validate_http_header_name("X-Custom-Header").is_ok());
        
        // Invalid header name  
        assert!(Validator::validate_http_header_name("X Header Space").is_err());
        assert!(Validator::validate_http_header_name("X-Header\nInjection").is_err());
        
        // Valid header value
        assert!(Validator::validate_http_header_value("application/json").is_ok());
        
        // Invalid header value
        assert!(Validator::validate_http_header_value("value\nwith\nnewline").is_err());
    }
}