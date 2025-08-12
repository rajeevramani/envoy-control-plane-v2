use super::errors::ConversionError;
use super::utils::{load_config_with_fallback, get_envoy_filter_name};
use crate::storage::models::HttpFilter as InternalHttpFilter;
use crate::xds::filters::FilterStrategyRegistry;
use prost::Message;
use prost_types::Any;
use tracing::info;

// Import Envoy protobuf types for listeners and HTTP filters
use envoy_types::pb::envoy::config::core::v3::{Address, SocketAddress};
use envoy_types::pb::envoy::config::listener::v3::{Filter, FilterChain, Listener};
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::{
    HttpConnectionManager, HttpFilter, Rds,
};
use envoy_types::pb::envoy::extensions::filters::http::router::v3::Router;

/// Convert listeners with HTTP filters to Envoy protobuf format
/// This function integrates with the FilterStrategyRegistry for HTTP filter conversion
pub fn listeners_to_proto(store: &crate::storage::ConfigStore) -> Result<Vec<Any>, ConversionError> {
    // Load config with fallback mechanism
    let app_config = load_config_with_fallback()?;

    info!("Listeners conversion: Creating main listener with HTTP filters");

    // Get all HTTP filters from store
    let http_filters = store.list_http_filters();
    let http_filters: Vec<InternalHttpFilter> = http_filters.iter().map(|f| (**f).clone()).collect();

    // Convert HTTP filters to Envoy format using FilterStrategyRegistry
    let envoy_http_filters = convert_http_filters(
        http_filters,
        &app_config.control_plane.http_filters.default_order,
        &app_config,
    )?;

    // Create HTTP Connection Manager with filters
    let http_conn_manager = HttpConnectionManager {
        stat_prefix: app_config.envoy_generation.http_filters.stat_prefix.clone(),
        route_specifier: Some(
            envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_connection_manager::RouteSpecifier::Rds(
                Rds {
                    config_source: Some(
                        envoy_types::pb::envoy::config::core::v3::ConfigSource {
                            config_source_specifier: Some(
                                envoy_types::pb::envoy::config::core::v3::config_source::ConfigSourceSpecifier::Ads(
                                    envoy_types::pb::envoy::config::core::v3::AggregatedConfigSource {
                                        ..Default::default()
                                    }
                                )
                            ),
                            resource_api_version: envoy_types::pb::envoy::config::core::v3::ApiVersion::V3 as i32,
                            ..Default::default()
                        }
                    ),
                    route_config_name: app_config.envoy_generation.naming.route_config_name.clone(),
                    ..Default::default()
                }
            )
        ),
        http_filters: envoy_http_filters,
        ..Default::default()
    };

    // Encode HTTP Connection Manager
    let mut hcm_buf = Vec::new();
    http_conn_manager.encode(&mut hcm_buf).map_err(|e| ConversionError::ProtobufEncoding {
        resource_type: "HttpConnectionManager".to_string(),
        source: e,
    })?;

    // Create main listener
    let listener = Listener {
        name: app_config.envoy_generation.bootstrap.main_listener_name.clone(),
        address: Some(Address {
            address: Some(envoy_types::pb::envoy::config::core::v3::address::Address::SocketAddress(
                SocketAddress {
                    protocol: protocol_to_proto(&app_config.envoy_generation.cluster.default_protocol),
                    address: app_config.envoy_generation.listener.binding_address.clone(),
                    port_specifier: Some(
                        envoy_types::pb::envoy::config::core::v3::socket_address::PortSpecifier::PortValue(
                            app_config.envoy_generation.listener.default_port as u32
                        )
                    ),
                    ..Default::default()
                }
            )),
        }),
        filter_chains: vec![FilterChain {
            filters: vec![Filter {
                name: app_config.envoy_generation.http_filters.hcm_filter_name.clone(),
                config_type: Some(
                    envoy_types::pb::envoy::config::listener::v3::filter::ConfigType::TypedConfig(
                        envoy_types::pb::google::protobuf::Any {
                            type_url: "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager".to_string(),
                            value: hcm_buf,
                        }
                    )
                ),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Encode listener
    let mut listener_buf = Vec::new();
    listener.encode(&mut listener_buf).map_err(|e| ConversionError::ProtobufEncoding {
        resource_type: "Listener".to_string(),
        source: e,
    })?;

    info!("✅ Listener conversion complete with {} HTTP filters integrated", 
          http_conn_manager.http_filters.len());

    Ok(vec![Any {
        type_url: "type.googleapis.com/envoy.config.listener.v3.Listener".to_string(),
        value: listener_buf,
    }])
}

/// Convert internal HTTP filters to Envoy protobuf HTTP filters using FilterStrategyRegistry
/// 🔧 This is the NEW implementation that replaces the old hardcoded match statements
pub fn convert_http_filters(
    http_filters: Vec<InternalHttpFilter>,
    default_order: &[String],
    app_config: &crate::config::AppConfig,
) -> Result<Vec<HttpFilter>, ConversionError> {
    let mut envoy_filters = Vec::new();

    // Create strategy registry with app config for configuration-dependent strategies
    let registry = FilterStrategyRegistry::new(app_config);
    info!("🔧 Using FilterStrategyRegistry with {} supported filter types", 
          registry.supported_filter_types().len());

    // Apply filters in the configured order
    for filter_type in default_order {
        // Find filters of this type
        let filters_of_type: Vec<&InternalHttpFilter> = http_filters
            .iter()
            .filter(|f| &f.filter_type == filter_type && f.enabled)
            .collect();

        for filter in filters_of_type {
            info!("🔄 Converting {} filter '{}' using strategy pattern", filter.filter_type, filter.name);
            
            // Use strategy registry to convert filter
            match registry.convert_filter(filter) {
                Ok(config_type) => {
                    // Get the appropriate Envoy filter name
                    let filter_name = get_envoy_filter_name(&filter.filter_type)?;
                    
                    envoy_filters.push(HttpFilter {
                        name: filter_name,
                        config_type: Some(config_type),
                        disabled: false,
                        is_optional: false,
                    });
                    
                    info!("✅ Successfully converted {} filter '{}'", filter.filter_type, filter.name);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    // Always add router filter last
    let router_config = Router::default();
    let mut buf = Vec::new();
    router_config.encode(&mut buf).map_err(|e| ConversionError::ProtobufEncoding {
        resource_type: "Router".to_string(),
        source: e,
    })?;

    envoy_filters.push(HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        config_type: Some(
            envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType::TypedConfig(
                envoy_types::pb::google::protobuf::Any {
                    type_url: "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router".to_string(),
                    value: buf,
                }
            )
        ),
        disabled: false,
        is_optional: false,
    });

    info!("🏁 HTTP filters conversion complete: {} filters created", envoy_filters.len());
    Ok(envoy_filters)
}

/// Convert protocol string to Envoy protobuf enum
fn protocol_to_proto(protocol: &str) -> i32 {
    use envoy_types::pb::envoy::config::core::v3::socket_address::Protocol;
    match protocol {
        "TCP" => Protocol::Tcp as i32,
        "UDP" => Protocol::Udp as i32,
        _ => {
            info!("Unknown protocol '{}', defaulting to TCP", protocol);
            Protocol::Tcp as i32
        }
    }
}