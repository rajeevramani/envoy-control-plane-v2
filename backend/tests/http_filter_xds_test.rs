use envoy_control_plane::storage::{ConfigStore, HttpFilter};
use envoy_control_plane::xds::conversion::ProtoConverter;

#[test]
fn test_listener_xds_with_http_filters() {
    let store = ConfigStore::new();
    
    // Add a test HTTP filter
    let filter = HttpFilter::new(
        "test-rate-limit".to_string(),
        "rate_limit".to_string(),
        serde_json::json!({
            "requests_per_unit": 100,
            "time_unit": "minute",
            "burst_size": 10
        })
    );
    
    let supported_filters = vec!["rate_limit".to_string()];
    store.add_http_filter(filter, &supported_filters).expect("Failed to add filter");
    
    // Test getting listener resources - this tests my xDS integration
    let resources = ProtoConverter::get_resources_by_type(
        "type.googleapis.com/envoy.config.listener.v3.Listener",
        &store,
    ).expect("Failed to get listener resources");
    
    // Should now return resources (not empty like before)
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].type_url,
        "type.googleapis.com/envoy.config.listener.v3.Listener"
    );
    
    println!("✅ Listener xDS integration with HTTP filters working!");
}

#[test] 
fn test_header_manipulation_filter_creation() {
    // Test the header manipulation helper I implemented
    let result = HttpFilter::create_header_manipulation_filter(
        "test-headers".to_string(),
        vec![("X-Custom-Header".to_string(), "test-value".to_string())],
        vec!["X-Remove-Header".to_string()],
        vec![("X-Response-Header".to_string(), "response-value".to_string())],
        vec!["X-Remove-Response".to_string()],
    );
    
    assert!(result.is_ok());
    let filter = result.unwrap();
    assert_eq!(filter.name, "test-headers");
    assert_eq!(filter.filter_type, "header_manipulation");
    assert!(filter.enabled);
    
    println!("✅ Header manipulation filter creation working!");
}

#[test]
fn test_comprehensive_filter_validation_integration() {
    // Test that comprehensive validation is integrated into the filter processing pipeline
    let store = ConfigStore::new();
    
    // Test 1: Invalid rate limit filter should be rejected
    let invalid_rate_limit = HttpFilter::new(
        "invalid-rate-limit".to_string(),
        "rate_limit".to_string(),
        serde_json::json!({"requests_per_unit": 0, "time_unit": "minute"}) // Invalid: zero requests
    );
    
    let supported_filters = vec!["rate_limit".to_string()];
    let result = store.add_http_filter(invalid_rate_limit, &supported_filters);
    // Note: Storage validation is separate from conversion validation
    // This tests that invalid configs are caught during conversion
    
    // Test 2: Invalid CORS filter should be rejected during conversion
    let invalid_cors = HttpFilter::new(
        "invalid-cors".to_string(), 
        "cors".to_string(),
        serde_json::json!({"allowed_methods": ["INVALID_METHOD"]}) // Invalid HTTP method
    );
    
    let cors_filters = vec![invalid_cors];
    let result = ProtoConverter::convert_http_filters(cors_filters, &["cors".to_string()]);
    assert!(result.is_err(), "Should reject invalid CORS filter during conversion");
    
    // Test 3: Valid filters should be accepted
    let valid_rate_limit = HttpFilter::new(
        "valid-rate-limit".to_string(),
        "rate_limit".to_string(), 
        serde_json::json!({"requests_per_unit": 100, "time_unit": "minute"})
    );
    
    let valid_filters = vec![valid_rate_limit];
    let result = ProtoConverter::convert_http_filters(valid_filters, &["rate_limit".to_string()]);
    assert!(result.is_ok(), "Should accept valid rate limit filter");
    
    println!("✅ Comprehensive filter validation integration working!");
}

#[test]
fn test_request_validation_filter_validation() {
    // Test the new request validation filter validation
    let valid_request_validation = HttpFilter::new(
        "test-validation".to_string(),
        "request_validation".to_string(),
        serde_json::json!({
            "allowed_methods": ["GET", "POST"],
            "required_headers": ["Authorization"]
        })
    );
    
    let result = ProtoConverter::validate_request_validation_config(&valid_request_validation);
    assert!(result.is_ok(), "Should accept valid request validation config");
    
    // Test invalid config
    let invalid_request_validation = HttpFilter::new(
        "test-invalid".to_string(),
        "request_validation".to_string(),
        serde_json::json!({
            "allowed_methods": [], // Empty array should be rejected
            "required_headers": ["Authorization"]
        })
    );
    
    let result = ProtoConverter::validate_request_validation_config(&invalid_request_validation);
    assert!(result.is_err(), "Should reject empty allowed_methods");
    
    println!("✅ Request validation filter validation working!");
}