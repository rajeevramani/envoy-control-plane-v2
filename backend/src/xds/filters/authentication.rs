use crate::storage::HttpFilter as InternalHttpFilter;
use crate::xds::conversion::ConversionError;
use crate::xds::filters::FilterStrategy;
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType;
use envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::{JwtAuthentication, JwtProvider, JwtRequirement};
use envoy_types::pb::google::protobuf::Any;
use std::collections::HashMap;
use tracing::info;
use base64::prelude::*;

/// Strategy for converting authentication filters to Envoy JWT Authentication
pub struct AuthenticationStrategy;

impl FilterStrategy for AuthenticationStrategy {
    fn filter_type(&self) -> &'static str {
        "authentication"
    }

    fn validate(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        // Validate JWT secret
        let jwt_secret = filter.config.get("jwt_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!("JWT secret for filter '{}' is missing", filter.name)
            })?;

        crate::validation::security::Validator::validate_jwt_secret(jwt_secret)
            .map_err(ConversionError::from)?;

        // Validate JWT issuer
        let jwt_issuer = filter.config.get("jwt_issuer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConversionError::ValidationFailed {
                reason: format!("JWT issuer for filter '{}' is missing", filter.name)
            })?;

        crate::validation::security::Validator::validate_length(jwt_issuer, "jwt_issuer", Some(1), Some(100))
            .map_err(ConversionError::from)?;

        Ok(())
    }

    fn convert(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError> {
        info!("Converting authentication filter '{}' to Envoy JWT Authentication", filter.name);

        // Extract JWT config from our JSON
        let jwt_secret = filter.config.get("jwt_secret")
            .and_then(|v| v.as_str())
            .unwrap_or("default-jwt-secret");

        let jwt_issuer = filter.config.get("jwt_issuer")
            .and_then(|v| v.as_str())
            .unwrap_or("https://default-issuer.com");

        let provider_name = format!("{}_provider", filter.name);

        // Create JWT provider (following existing pattern)
        let jwt_provider = JwtProvider {
            issuer: jwt_issuer.to_string(),
            jwt_cache_config: Some(envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::JwtCacheConfig {
                jwt_cache_size: 1000,
                ..Default::default()
            }),
            jwks_source_specifier: Some(
                envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::jwt_provider::JwksSourceSpecifier::LocalJwks(
                    envoy_types::pb::envoy::config::core::v3::DataSource {
                        specifier: Some(envoy_types::pb::envoy::config::core::v3::data_source::Specifier::InlineString(
                            format!(r#"{{"keys":[{{"kty":"oct","k":"{}"}}]}}"#, 
                                base64::prelude::BASE64_STANDARD.encode(jwt_secret))
                        )),
                        watched_directory: None,
                    }
                )
            ),
            ..Default::default()
        };

        // Create providers map
        let mut providers = HashMap::new();
        providers.insert(provider_name.clone(), jwt_provider);

        // Create requirement
        let requirement = JwtRequirement {
            requires_type: Some(
                envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::jwt_requirement::RequiresType::ProviderName(
                    provider_name.clone()
                )
            ),
        };

        // Create JWT authentication configuration (following existing pattern)
        let jwt_auth_config = JwtAuthentication {
            providers,
            rules: vec![
                envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::RequirementRule {
                    r#match: Some(envoy_types::pb::envoy::config::route::v3::RouteMatch {
                        path_specifier: Some(
                            envoy_types::pb::envoy::config::route::v3::route_match::PathSpecifier::Prefix("/".to_string())
                        ),
                        ..Default::default()
                    }),
                    requirement_type: Some(
                        envoy_types::pb::envoy::extensions::filters::http::jwt_authn::v3::requirement_rule::RequirementType::Requires(
                            requirement
                        )
                    ),
                    ..Default::default()
                }
            ],
            ..Default::default()
        };

        // Serialize to Any proto
        let any_config = Any {
            type_url: "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication".to_string(),
            value: {
                let mut buf = Vec::new();
                prost::Message::encode(&jwt_auth_config, &mut buf)
                    .map_err(|e| ConversionError::ProtobufEncoding {
                        resource_type: "JwtAuthentication".to_string(),
                        source: e,
                    })?;
                buf
            },
        };

        Ok(ConfigType::TypedConfig(any_config))
    }

    fn description(&self) -> &'static str {
        "JWT authentication filter for validating JSON Web Tokens"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_authentication_validation() {
        let strategy = AuthenticationStrategy;
        
        // Valid configuration
        let valid_filter = InternalHttpFilter {
            name: "test-auth".to_string(),
            filter_type: "authentication".to_string(),
            enabled: true,
            config: json!({
                "jwt_secret": "this-is-a-very-secure-jwt-key-with-sufficient-length-for-validation",
                "jwt_issuer": "https://auth.example.com"
            }),
        };
        
        assert!(strategy.validate(&valid_filter).is_ok());
        
        // Invalid configuration - missing JWT secret
        let invalid_filter = InternalHttpFilter {
            name: "test-invalid".to_string(),
            filter_type: "authentication".to_string(),
            enabled: true,
            config: json!({
                "jwt_issuer": "https://auth.example.com"
            }),
        };
        
        assert!(strategy.validate(&invalid_filter).is_err());
    }

    #[test]
    fn test_authentication_conversion() {
        let strategy = AuthenticationStrategy;
        
        let filter = InternalHttpFilter {
            name: "test-auth".to_string(),
            filter_type: "authentication".to_string(),
            enabled: true,
            config: json!({
                "jwt_secret": "my-super-secure-jwt-key-with-sufficient-length-for-validation",
                "jwt_issuer": "https://auth.example.com"
            }),
        };
        
        let result = strategy.convert(&filter);
        assert!(result.is_ok());
        
        if let Ok(ConfigType::TypedConfig(any)) = result {
            assert_eq!(any.type_url, "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication");
        } else {
            panic!("Expected TypedConfig result");
        }
    }

    #[test]
    fn test_authentication_weak_secret_validation() {
        let strategy = AuthenticationStrategy;
        
        // Weak JWT secret should be rejected
        let invalid_filter = InternalHttpFilter {
            name: "test-weak".to_string(),
            filter_type: "authentication".to_string(),
            enabled: true,
            config: json!({
                "jwt_secret": "my-secret-key", // Contains "secret" - should be rejected
                "jwt_issuer": "https://auth.example.com"
            }),
        };
        
        let result = strategy.validate(&invalid_filter);
        assert!(result.is_err(), "Should reject weak JWT secret");
    }
}