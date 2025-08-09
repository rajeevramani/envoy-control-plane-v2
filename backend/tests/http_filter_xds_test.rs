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
            "unit": "minute",
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