use envoy_control_plane::storage::HttpFilter;
use envoy_control_plane::xds::conversion::{ProtoConverter, ConversionError};
use serde_json::json;

#[test]
fn test_safe_lua_string_escaping() {
    let test_cases = vec![
        // Basic escaping
        ("hello", "\"hello\""),
        ("hello world", "\"hello world\""),
        
        // Special character escaping
        ("quote\"test", "\"quote\\\"test\""),
        ("backslash\\test", "\"backslash\\\\test\""),
        ("newline\ntest", "\"newline\\ntest\""),
        ("tab\ttest", "\"tab\\ttest\""),
        ("return\rtest", "\"return\\rtest\""),
        
        // Combined escaping (without null character which is detected as dangerous)
        ("\"hello\"\n\\world\\", "\"\\\"hello\\\"\\n\\\\world\\\\\""),
    ];

    for (input, expected) in test_cases {
        let result = ProtoConverter::safe_lua_string(input, "test_field");
        match result {
            Ok(escaped) => assert_eq!(escaped, expected, "Failed for input: '{}'", input),
            Err(e) => panic!("Unexpected error for safe input '{}': {:?}", input, e),
        }
    }
}

#[test] 
fn test_lua_injection_detection() {
    let dangerous_inputs = vec![
        // Lua execution functions
        "os.execute('rm -rf /')",
        "io.popen('cat /etc/passwd')",
        "loadstring('malicious code')",
        "load(malicious_function)",
        "dofile('/etc/passwd')",
        "loadfile('evil.lua')",
        
        // Debug and system access  
        "debug.getlocal()",
        "package.loadlib()",
        "require('malicious')",
        "_G['os']['execute']",
        "getfenv()",
        "setfenv()",
        
        // Script manipulation
        "]] .. malicious_code .. [[",
        ".. [[ injection",  
        "\nend\nfunction evil()",
        "\nfunction malicious()",
        "\nlocal evil = true",
        "\nif true then os.execute()",
        "\nfor i=1,10 do malicious() end",
        "\nwhile true do attack() end",
        "\nrepeat attack() until false",
        "\ndo\n malicious_code()",
        
        // Comment injection
        "--]] malicious ]]--",
        "]]-- injection",
        "/* comment injection */",
        "*/malicious/*",
        
        // Script termination
        "test\0injection",
        "test\\0injection", 
        "\nreturn malicious()",
        "return\nevil_code()",
    ];

    for dangerous_input in dangerous_inputs {
        let result = ProtoConverter::safe_lua_string(dangerous_input, "test_field");
        assert!(result.is_err(), "Should reject dangerous input: '{}'", dangerous_input);
        
        match result {
            Err(ConversionError::ValidationFailed { reason }) => {
                assert!(reason.contains("dangerous Lua pattern") || reason.contains("injection attempt"));
            }
            _ => panic!("Expected ValidationFailed error for: '{}'", dangerous_input),
        }
    }
}

#[test]
fn test_header_name_validation() {
    // Valid header names
    let valid_names = vec![
        "Content-Type",
        "Authorization", 
        "X-Custom-Header",
        "Cache_Control",
        "Accept.Language",
        "A", // Single character
    ];
    
    // Test maximum length separately due to String vs &str issues
    let max_length_name = "a".repeat(100);
    let result = ProtoConverter::validate_header_name(&max_length_name);
    assert!(result.is_ok(), "Should accept maximum length header name");

    for name in valid_names {
        let result = ProtoConverter::validate_header_name(&name);
        assert!(result.is_ok(), "Should accept valid header name: '{}'", name);
    }

    // Invalid header names
    let invalid_names = vec![
        ("", "empty"),
        ("X-Header\nInjection", "newline injection"),
        ("X-Header\0Null", "null injection"),
        ("X-Header os.execute", "lua injection"),
        ("X-Header--]]", "comment injection"),
        ("X Header Space", "space not allowed"),
        ("X-Header@Special", "invalid character"),
    ];

    for (name, description) in invalid_names {
        let result = ProtoConverter::validate_header_name(name);
        assert!(result.is_err(), "Should reject invalid header name ({}): '{}'", description, name);
    }
    
    // Test too-long header name separately
    let too_long_name = "a".repeat(101);
    let result = ProtoConverter::validate_header_name(&too_long_name);
    assert!(result.is_err(), "Should reject too-long header name");
}

#[test]
fn test_header_value_validation() {
    // Valid header values
    let valid_values = vec![
        "application/json",
        "Bearer token123",
        "value with spaces",
        "value\twith\ttab", // Tab is allowed
        "",  // Empty is valid
    ];
    
    // Test maximum length separately
    let max_length_value = "a".repeat(8192);
    let result = ProtoConverter::validate_header_value(&max_length_value);
    assert!(result.is_ok(), "Should accept maximum length header value");

    for value in valid_values {
        let result = ProtoConverter::validate_header_value(&value);
        assert!(result.is_ok(), "Should accept valid header value: '{}'", value);
    }

    // Invalid header values
    let invalid_values = vec![
        ("value\nwith\nnewline", "newline not allowed"),
        ("value\rwith\rreturn", "carriage return not allowed"), 
        ("value\0with\0null", "null not allowed"),
        ("value os.execute", "lua injection"),
        ("value--]]injection", "comment injection"),
    ];

    for (value, description) in invalid_values {
        let result = ProtoConverter::validate_header_value(value);
        assert!(result.is_err(), "Should reject invalid header value ({}): '{}'", description, value);
    }
    
    // Test too-long header value separately
    let too_long_value = "a".repeat(8193);
    let result = ProtoConverter::validate_header_value(&too_long_value);
    assert!(result.is_err(), "Should reject too-long header value");
}

#[test]
fn test_control_character_limits() {
    // Should allow reasonable control characters
    let reasonable_input = "line1\nline2\tindented";
    let result = ProtoConverter::safe_lua_string(reasonable_input, "test");
    assert!(result.is_ok(), "Should allow reasonable control characters");

    // Should reject excessive control characters (potential binary injection)
    // Using control chars that aren't in our dangerous patterns list
    let excessive_control = "test\x01\x02\x03\x04\x05excessive"; // 5 control chars (more than our limit of 2)
    let result = ProtoConverter::safe_lua_string(excessive_control, "test");
    assert!(result.is_err(), "Should reject excessive control characters");
    
    match result {
        Err(ConversionError::ValidationFailed { reason }) => {
            assert!(reason.contains("excessive control characters") || reason.contains("control character"));
        }
        _ => panic!("Expected ValidationFailed for excessive control characters"),
    }
}

#[test] 
fn test_header_manipulation_filter_security() {
    // Test that header manipulation filter properly validates input
    let malicious_filter = HttpFilter::new(
        "malicious-header".to_string(),
        "header_manipulation".to_string(),
        json!({
            "request_headers_to_add": [
                {
                    "header": {
                        "key": "X-Malicious",
                        "value": "normal\"; os.execute('rm -rf /'); --"
                    }
                }
            ]
        })
    );

    // Converting this filter should fail due to Lua injection in header value
    let result = ProtoConverter::convert_http_filters(vec![malicious_filter.clone()], &["header_manipulation".to_string()]);
    assert!(result.is_err(), "Should reject filter with malicious header value");

    // Test malicious header name
    let malicious_name_filter = HttpFilter::new(
        "malicious-name".to_string(), 
        "header_manipulation".to_string(),
        json!({
            "request_headers_to_add": [
                {
                    "header": {
                        "key": "X-Header\nend\nfunction evil()\nos.execute('evil')\n--",
                        "value": "safe-value"
                    }
                }
            ]
        })
    );

    let result = ProtoConverter::convert_http_filters(vec![malicious_name_filter.clone()], &["header_manipulation".to_string()]);
    assert!(result.is_err(), "Should reject filter with malicious header name");
}

#[test]
fn test_safe_header_manipulation_filter() {
    // Test that safe header manipulation works correctly
    let safe_filter = HttpFilter::new(
        "safe-header".to_string(),
        "header_manipulation".to_string(), 
        json!({
            "request_headers_to_add": [
                {
                    "header": {
                        "key": "X-Safe-Header",
                        "value": "Safe Value With \"Quotes\" and \\backslashes\\"
                    }
                }
            ],
            "request_headers_to_remove": ["X-Remove-Header"],
            "response_headers_to_add": [
                {
                    "header": {
                        "key": "X-Response-Header", 
                        "value": "Response Value"
                    }
                }
            ],
            "response_headers_to_remove": ["X-Remove-Response"]
        })
    );

    let result = ProtoConverter::convert_http_filters(vec![safe_filter.clone()], &["header_manipulation".to_string()]);
    assert!(result.is_ok(), "Should accept filter with safe header values");

    // Verify that the generated Lua script contains properly escaped strings
    if let Ok(filters) = result {
        assert!(!filters.is_empty(), "Should generate at least one filter");
        // The actual Lua script would be embedded in the protobuf, 
        // but we can't easily inspect it here without additional test infrastructure
    }
}

#[test]
fn test_case_insensitive_injection_detection() {
    let mixed_case_attacks = vec![
        "OS.EXECUTE('attack')",
        "Os.Execute('attack')", 
        "oS.eXeCuTe('attack')",
        "IO.POPEN('attack')",
        "LOADSTRING('attack')",
        "DEBUG.getlocal()",
        "]] .. MALICIOUS .. [[",
        "\nEND\nFUNCTION evil()",
    ];

    for attack in mixed_case_attacks {
        let result = ProtoConverter::safe_lua_string(attack, "test");
        assert!(result.is_err(), "Should detect case-insensitive injection: '{}'", attack);
    }
}