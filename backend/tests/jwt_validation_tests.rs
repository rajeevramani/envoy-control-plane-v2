use envoy_control_plane::storage::HttpFilter;
use envoy_control_plane::xds::conversion::{ProtoConverter, ConversionError};
use serde_json::json;

#[test]
fn test_jwt_secret_validation_success() {
    let valid_secret = "Kj8N9mL2pQ7rS1tU4vW8xY0zA3bC6dE9fGh5JkMnPq2sT6uV9yB";
    
    let filter = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": valid_secret,
            "jwt_issuer": "test-issuer"
        })
    );

    // This should not panic and should validate successfully
    let result = ProtoConverter::validate_jwt_auth_config(&filter);
    match result {
        Ok(_) => {},
        Err(e) => panic!("Valid JWT config should pass validation, but got error: {:?}", e),
    }
}

#[test]
fn test_jwt_secret_too_short() {
    let short_secret = "short_secret"; // Only 12 characters
    
    let filter = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": short_secret,
            "jwt_issuer": "test-issuer"
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter);
    assert!(result.is_err(), "Short JWT secret should fail validation");
    
    match result {
        Err(ConversionError::ValidationFailed { reason }) => {
            assert!(reason.contains("at least 32 characters"));
            assert!(reason.contains("test-jwt"));
        }
        _ => panic!("Expected ValidationFailed error"),
    }
}

#[test]
fn test_jwt_secret_empty() {
    let filter = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": "",
            "jwt_issuer": "test-issuer"
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter);
    assert!(result.is_err(), "Empty JWT secret should fail validation");
    
    match result {
        Err(ConversionError::ValidationFailed { reason }) => {
            assert!(reason.contains("cannot be empty"));
        }
        _ => panic!("Expected ValidationFailed error"),
    }
}

#[test]
fn test_jwt_secret_missing() {
    let filter = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_issuer": "test-issuer"
            // jwt_secret is missing
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter);
    assert!(result.is_err(), "Missing JWT secret should fail validation");
}

#[test]
fn test_jwt_secret_weak_patterns() {
    let weak_secrets = vec![
        "this_contains_the_word_secret_and_is_long_enough",
        "this_contains_the_word_password_and_is_long_enough", 
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", // All 'a's
        "11111111111111111111111111111111111", // All '1's
    ];

    for weak_secret in weak_secrets {
        let filter = HttpFilter::new(
            "test-jwt".to_string(),
            "authentication".to_string(),
            json!({
                "jwt_secret": weak_secret,
                "jwt_issuer": "test-issuer"
            })
        );

        let result = ProtoConverter::validate_jwt_auth_config(&filter);
        assert!(result.is_err(), "Weak JWT secret '{}' should fail validation", weak_secret);
        
        match result {
            Err(ConversionError::ValidationFailed { reason }) => {
                assert!(reason.contains("appears to be weak") || reason.contains("obvious patterns"));
            }
            _ => panic!("Expected ValidationFailed error for weak secret: {}", weak_secret),
        }
    }
}

#[test]
fn test_jwt_issuer_validation() {
    let valid_secret = "Kj8N9mL2pQ7rS1tU4vW8xY0zA3bC6dE9fGh5JkMnPq2sT6uV9yB";
    
    // Test empty issuer
    let filter_empty_issuer = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": valid_secret,
            "jwt_issuer": ""
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter_empty_issuer);
    assert!(result.is_err(), "Empty JWT issuer should fail validation");

    // Test missing issuer
    let filter_missing_issuer = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": valid_secret
            // jwt_issuer is missing
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter_missing_issuer);
    assert!(result.is_err(), "Missing JWT issuer should fail validation");

    // Test issuer too long (over 100 characters)
    let long_issuer = "a".repeat(101);
    let filter_long_issuer = HttpFilter::new(
        "test-jwt".to_string(),
        "authentication".to_string(),
        json!({
            "jwt_secret": valid_secret,
            "jwt_issuer": &long_issuer
        })
    );

    let result = ProtoConverter::validate_jwt_auth_config(&filter_long_issuer);
    assert!(result.is_err(), "JWT issuer over 100 characters should fail validation");

    match result {
        Err(ConversionError::ValidationFailed { reason }) => {
            assert!(reason.contains("at most 100 characters"));
        }
        _ => panic!("Expected ValidationFailed error"),
    }
}

#[test]
fn test_jwt_issuer_valid_lengths() {
    let valid_secret = "Kj8N9mL2pQ7rS1tU4vW8xY0zA3bC6dE9fGh5JkMnPq2sT6uV9yB";
    
    // Test valid issuer lengths
    let long_issuer = "a".repeat(100);
    let valid_issuers = vec![
        "a",                          // 1 character
        "test-issuer",                // Normal length
        &long_issuer,                 // Exactly 100 characters (max allowed)
    ];

    for issuer in valid_issuers {
        let filter = HttpFilter::new(
            "test-jwt".to_string(),
            "authentication".to_string(),
            json!({
                "jwt_secret": valid_secret,
                "jwt_issuer": issuer
            })
        );

        let result = ProtoConverter::validate_jwt_auth_config(&filter);
        assert!(result.is_ok(), "Valid JWT issuer '{}' should pass validation", issuer);
    }
}

#[test]
fn test_secure_jwt_secret_examples() {
    let secure_secrets = vec![
        "Kj8N9mL2pQ7rS1tU4vW8xY0zA3bC6dE9f", // Random-looking 32 chars
        "VeryLongAndStrongJWTKeyWithRandomElements32Chars", // Descriptive but secure
        "JWT-KEY-2024-PROD-ENV-SECURE-RANDOM-STRING-ABCD1234", // Production-like
        "ZmYxNjA5N2UtOGUwNi00M2E4LWJjYzktODNmYzMwNzY4NDUw", // Base64-like
    ];

    for secure_secret in secure_secrets {
        let filter = HttpFilter::new(
            "test-jwt".to_string(),
            "authentication".to_string(),
            json!({
                "jwt_secret": secure_secret,
                "jwt_issuer": "test-issuer"
            })
        );

        let result = ProtoConverter::validate_jwt_auth_config(&filter);
        assert!(result.is_ok(), "Secure JWT secret '{}' should pass validation", secure_secret);
    }
}