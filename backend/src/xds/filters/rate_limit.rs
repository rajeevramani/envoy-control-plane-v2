use crate::storage::HttpFilter as InternalHttpFilter;
use crate::xds::conversion::ConversionError;
use crate::xds::filters::FilterStrategy;
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType;
use envoy_types::pb::envoy::extensions::filters::http::local_ratelimit::v3::LocalRateLimit;
use envoy_types::pb::google::protobuf::Any;
use tracing::info;

/// Strategy for converting rate limit filters to Envoy LocalRateLimit
pub struct RateLimitStrategy;

impl FilterStrategy for RateLimitStrategy {
    fn filter_type(&self) -> &'static str {
        "rate_limit"
    }

    fn validate(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        // Validate requests_per_unit (required field)
        let requests_per_unit = filter.config.get("requests_per_unit")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!("Rate limit 'requests_per_unit' for filter '{}' is missing or invalid. Must be a positive integer.", filter.name)
            })?;

        if requests_per_unit == 0 || requests_per_unit > 1000000 {
            return Err(ConversionError::ValidationFailed {
                reason: format!("Rate limit 'requests_per_unit' must be between 1 and 1,000,000 for filter '{}', got: {}", filter.name, requests_per_unit)
            });
        }

        // Validate time_unit (required field)
        let time_unit = filter.config.get("time_unit")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!("Rate limit 'time_unit' for filter '{}' is missing. Must be one of: second, minute, hour, day", filter.name)
            })?;

        match time_unit {
            "second" | "minute" | "hour" | "day" => {},
            _ => return Err(ConversionError::ValidationFailed {
                reason: format!("Invalid 'time_unit' '{}' for filter '{}'. Must be one of: second, minute, hour, day", 
                    time_unit, filter.name)
            })
        }

        // Validate burst_size (optional field) and its relationship with requests_per_unit
        if let Some(burst_size_value) = filter.config.get("burst_size") {
            let burst_size = burst_size_value.as_u64()
                .ok_or_else(|| ConversionError::ValidationFailed {
                    reason: format!("Rate limit 'burst_size' for filter '{}' is invalid. Must be a positive integer.", filter.name)
                })?;

            if burst_size == 0 {
                return Err(ConversionError::ValidationFailed {
                    reason: format!("Rate limit 'burst_size' must be greater than 0 for filter '{}', got: {}", filter.name, burst_size)
                });
            }

            // Architecture advisor's key insight: burst_size must be >= requests_per_unit
            // This ensures max_tokens >= tokens_per_fill in the token bucket
            if burst_size < requests_per_unit {
                return Err(ConversionError::ValidationFailed {
                    reason: format!(
                        "Rate limit 'burst_size' ({}) cannot be less than 'requests_per_unit' ({}) for filter '{}'. \
                        Burst size represents peak capacity, which must be >= sustained rate.",
                        burst_size, requests_per_unit, filter.name
                    )
                });
            }
        }

        // Check for invalid/unsupported field names to prevent configuration errors
        let valid_fields = ["requests_per_unit", "time_unit", "burst_size"];
        for (key, _) in filter.config.as_object().unwrap_or(&serde_json::Map::new()) {
            if !valid_fields.contains(&key.as_str()) {
                return Err(ConversionError::ValidationFailed {
                    reason: format!(
                        "Invalid field '{}' in rate limit filter '{}'. Valid fields are: {}",
                        key, filter.name, valid_fields.join(", ")
                    )
                });
            }
        }

        Ok(())
    }

    fn convert(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError> {
        info!("Converting rate_limit filter '{}' to Envoy LocalRateLimit", filter.name);

        // Extract rate limiting config from our JSON
        let requests_per_unit = filter.config.get("requests_per_unit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        
        let time_unit_str = filter.config.get("time_unit")
            .and_then(|v| v.as_str())
            .unwrap_or("minute");
            
        let burst_size = filter.config.get("burst_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        // Convert time unit to seconds for Envoy
        let unit_duration_seconds = match time_unit_str {
            "second" => 1,
            "minute" => 60,
            "hour" => 3600,
            "day" => 86400,
            _ => 60, // Default to minute
        };

        // Create token bucket configuration
        let mut token_bucket = envoy_types::pb::envoy::r#type::v3::TokenBucket {
            max_tokens: requests_per_unit,
            tokens_per_fill: Some(envoy_types::pb::google::protobuf::UInt32Value {
                value: requests_per_unit,
            }),
            fill_interval: Some(envoy_types::pb::google::protobuf::Duration {
                seconds: unit_duration_seconds as i64,
                nanos: 0,
            }),
        };

        // Set burst size if specified
        if let Some(burst) = burst_size {
            token_bucket.max_tokens = burst;
        }

        // Create LocalRateLimit configuration following official Envoy documentation
        // Need to explicitly enable the filter, otherwise it defaults to 0% (disabled)
        let local_rate_limit = LocalRateLimit {
            stat_prefix: format!("rate_limit_{}", filter.name),
            token_bucket: Some(token_bucket),
            // Enable the filter for 100% of requests to make it functional
            filter_enabled: Some(envoy_types::pb::envoy::config::core::v3::RuntimeFractionalPercent {
                default_value: Some(envoy_types::pb::envoy::r#type::v3::FractionalPercent {
                    numerator: 100,
                    denominator: envoy_types::pb::envoy::r#type::v3::fractional_percent::DenominatorType::Hundred as i32,
                }),
                ..Default::default()
            }),
            // Enforce the rate limit for 100% of checked requests
            filter_enforced: Some(envoy_types::pb::envoy::config::core::v3::RuntimeFractionalPercent {
                default_value: Some(envoy_types::pb::envoy::r#type::v3::FractionalPercent {
                    numerator: 100,
                    denominator: envoy_types::pb::envoy::r#type::v3::fractional_percent::DenominatorType::Hundred as i32,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        // DEBUG: Export the LocalRateLimit configuration for analysis
        info!("🔍 LocalRateLimit Configuration Debug:");
        info!("  stat_prefix: {}", local_rate_limit.stat_prefix);
        if let Some(ref bucket) = local_rate_limit.token_bucket {
            info!("  token_bucket.max_tokens: {}", bucket.max_tokens);
            info!("  token_bucket.tokens_per_fill: {:?}", bucket.tokens_per_fill.as_ref().map(|v| v.value));
            info!("  token_bucket.fill_interval: {:?}", bucket.fill_interval.as_ref().map(|d| format!("{}s", d.seconds)));
        }
        if let Some(ref enabled) = local_rate_limit.filter_enabled {
            if let Some(ref default) = enabled.default_value {
                info!("  filter_enabled: {}% (numerator: {}, denominator: {})", 
                    (default.numerator as f64 / 100.0) * 100.0, default.numerator, default.denominator);
            }
        }
        if let Some(ref enforced) = local_rate_limit.filter_enforced {
            if let Some(ref default) = enforced.default_value {
                info!("  filter_enforced: {}% (numerator: {}, denominator: {})", 
                    (default.numerator as f64 / 100.0) * 100.0, default.numerator, default.denominator);
            }
        }
        
        // Serialize to Any proto
        let any_config = Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit".to_string(),
            value: {
                let mut buf = Vec::new();
                prost::Message::encode(&local_rate_limit, &mut buf)
                    .map_err(|e| ConversionError::ProtobufEncoding {
                        resource_type: "LocalRateLimit".to_string(),
                        source: e,
                    })?;
                buf
            },
        };
        
        info!("🔍 Protobuf Any Config:");
        info!("  type_url: {}", any_config.type_url);
        info!("  value.len(): {} bytes", any_config.value.len());

        Ok(ConfigType::TypedConfig(any_config))
    }

    fn description(&self) -> &'static str {
        "Rate limiting filter using Envoy's LocalRateLimit with token bucket algorithm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_rate_limit_validation() {
        let strategy = RateLimitStrategy;
        
        // Valid configuration without burst_size
        let valid_filter = InternalHttpFilter {
            name: "test-rate-limit".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 100,
                "time_unit": "minute"
            }),
        };
        assert!(strategy.validate(&valid_filter).is_ok());
        
        // Valid configuration with burst_size >= requests_per_unit
        let valid_burst_filter = InternalHttpFilter {
            name: "test-burst".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 50,
                "time_unit": "minute",
                "burst_size": 100
            }),
        };
        assert!(strategy.validate(&valid_burst_filter).is_ok());
        
        // Invalid: missing requests_per_unit
        let missing_rate = InternalHttpFilter {
            name: "test-missing-rate".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({"time_unit": "minute"}),
        };
        assert!(strategy.validate(&missing_rate).is_err());
        
        // Invalid: missing time_unit
        let missing_time_unit = InternalHttpFilter {
            name: "test-missing-time".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({"requests_per_unit": 100}),
        };
        assert!(strategy.validate(&missing_time_unit).is_err());
        
        // Invalid: burst_size < requests_per_unit
        let invalid_burst = InternalHttpFilter {
            name: "test-invalid-burst".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 100,
                "time_unit": "minute",
                "burst_size": 50
            }),
        };
        let result = strategy.validate(&invalid_burst);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be less than"));
        
        // Invalid: wrong field name (unit instead of time_unit)
        let wrong_field = InternalHttpFilter {
            name: "test-wrong-field".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 100,
                "unit": "minute"  // Should be "time_unit"
            }),
        };
        let result = strategy.validate(&wrong_field);
        assert!(result.is_err());
        // This fails because time_unit is missing (unit != time_unit)
        assert!(result.unwrap_err().to_string().contains("time_unit"));
        
        // Invalid: extra unsupported field
        let extra_field = InternalHttpFilter {
            name: "test-extra-field".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 100,
                "time_unit": "minute",
                "invalid_extra_field": "should_not_be_here"
            }),
        };
        let result = strategy.validate(&extra_field);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid field 'invalid_extra_field'"));
    }

    #[test]
    fn test_rate_limit_conversion() {
        let strategy = RateLimitStrategy;
        
        let filter = InternalHttpFilter {
            name: "test-rate-limit".to_string(),
            filter_type: "rate_limit".to_string(),
            enabled: true,
            config: json!({
                "requests_per_unit": 100,
                "time_unit": "minute",
                "burst_size": 150
            }),
        };
        
        let result = strategy.convert(&filter);
        assert!(result.is_ok());
        
        if let Ok(ConfigType::TypedConfig(any)) = result {
            assert_eq!(any.type_url, "type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit");
        } else {
            panic!("Expected TypedConfig result");
        }
    }
}