use super::errors::ConversionError;
use super::utils::{load_config_with_fallback, validate_route};
use crate::storage::models::Route as InternalRoute;
use prost::Message;
use prost_types::Any;
use tracing::info;

// Import Envoy protobuf types for routes
use envoy_types::pb::envoy::config::route::v3::{
    HeaderMatcher, Route, RouteAction, RouteConfiguration, RouteMatch, VirtualHost,
};
use envoy_types::pb::envoy::r#type::matcher::v3::{RegexMatcher, StringMatcher};

/// Convert internal routes to Envoy protobuf format
pub fn routes_to_proto(routes: Vec<InternalRoute>) -> Result<Vec<Any>, ConversionError> {
    if routes.is_empty() {
        return Ok(vec![]);
    }

    // Load config with fallback mechanism
    let app_config = load_config_with_fallback()?;

    info!(
        "Routes conversion: Creating RouteConfiguration with {} routes",
        routes.len()
    );

    // Create routes following the Go control plane pattern with validation
    let mut proto_routes = Vec::new();
    
    for route in routes {
        // Validate route before conversion
        validate_route(&route)?;
        
        info!("  - Route: {} -> {}", route.path, route.cluster_name);

        // Create header matchers for HTTP methods if specified
        let headers = if let Some(ref methods) = route.http_methods {
            if methods.len() == 1 {
                // Single method - use string match with exact
                vec![HeaderMatcher {
                    name: ":method".to_string(),
                    header_match_specifier: Some(
                        envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::StringMatch(
                            StringMatcher {
                                match_pattern: Some(
                                    envoy_types::pb::envoy::r#type::matcher::v3::string_matcher::MatchPattern::Exact(methods[0].clone())
                                ),
                                ..Default::default()
                            }
                        )
                    ),
                    ..Default::default()
                }]
            } else {
                // Multiple methods - use regex match
                let regex_pattern = format!("^({})$", methods.join("|"));
                vec![HeaderMatcher {
                    name: ":method".to_string(),
                    header_match_specifier: Some(
                        envoy_types::pb::envoy::config::route::v3::header_matcher::HeaderMatchSpecifier::SafeRegexMatch(
                            RegexMatcher {
                                regex: regex_pattern,
                                ..Default::default()
                            }
                        )
                    ),
                    ..Default::default()
                }]
            }
        } else {
            // No HTTP methods specified - match all methods
            vec![]
        };

        let proto_route = Route {
            r#match: Some(RouteMatch {
                path_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_match::PathSpecifier::Prefix(route.path)),
                headers,
                ..Default::default()
            }),
            action: Some(envoy_types::pb::envoy::config::route::v3::route::Action::Route(RouteAction {
                cluster_specifier: Some(envoy_types::pb::envoy::config::route::v3::route_action::ClusterSpecifier::Cluster(route.cluster_name)),
                prefix_rewrite: route.prefix_rewrite.unwrap_or_default(),
                ..Default::default()
            })),
            ..Default::default()
        };
        
        proto_routes.push(proto_route);
    }

    // Create virtual host with all routes
    let virtual_host = VirtualHost {
        name: app_config.envoy_generation.naming.virtual_host_name.clone(),
        domains: app_config.envoy_generation.naming.default_domains.clone(),
        routes: proto_routes,
        ..Default::default()
    };

    // Create RouteConfiguration
    let route_config = RouteConfiguration {
        name: app_config.envoy_generation.naming.route_config_name.clone(),
        virtual_hosts: vec![virtual_host],
        ..Default::default()
    };

    // Encode to protobuf bytes with proper error handling
    let mut buf = Vec::new();
    route_config.encode(&mut buf)
        .map_err(|e| ConversionError::ProtobufEncoding {
            resource_type: "RouteConfiguration".to_string(),
            source: e,
        })?;

    Ok(vec![Any {
        type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
        value: buf,
    }])
}