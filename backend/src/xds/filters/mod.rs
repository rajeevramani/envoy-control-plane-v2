use crate::config::AppConfig;
use crate::storage::HttpFilter as InternalHttpFilter;
use crate::xds::conversion::ConversionError;
use envoy_types::pb::envoy::extensions::filters::network::http_connection_manager::v3::http_filter::ConfigType;

pub mod rate_limit;
pub mod cors;
pub mod authentication;
pub mod header_manipulation;
pub mod request_validation;

pub use rate_limit::RateLimitStrategy;
pub use cors::CorsStrategy;
pub use authentication::AuthenticationStrategy;
pub use header_manipulation::HeaderManipulationStrategy;
pub use request_validation::RequestValidationStrategy;

/// Strategy pattern trait for converting different HTTP filter types to Envoy protobuf
/// 
/// This trait defines a common interface for all filter conversion strategies,
/// making it easy to add new filter types and maintain existing ones.
pub trait FilterStrategy {
    /// Get the filter type name that this strategy handles
    fn filter_type(&self) -> &'static str;
    
    /// Validate the filter configuration before conversion
    fn validate(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError>;
    
    /// Convert the internal filter to Envoy protobuf configuration
    fn convert(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError>;
    
    /// Get a human-readable description of what this filter does
    fn description(&self) -> &'static str;
    
    /// Check if this strategy supports the given filter type
    fn supports(&self, filter_type: &str) -> bool {
        self.filter_type() == filter_type
    }
}

/// Registry of all available filter strategies
pub struct FilterStrategyRegistry {
    strategies: Vec<Box<dyn FilterStrategy>>,
}

impl FilterStrategyRegistry {
    /// Create a new registry with all available filter strategies
    pub fn new(app_config: &AppConfig) -> Self {
        let mut registry = Self {
            strategies: Vec::new(),
        };
        
        // Register all built-in filter strategies
        registry.register_builtin_strategies(app_config);
        registry
    }
    
    /// Register all built-in filter strategies
    fn register_builtin_strategies(&mut self, app_config: &AppConfig) {
        self.register(Box::new(RateLimitStrategy));
        self.register(Box::new(CorsStrategy)); 
        self.register(Box::new(AuthenticationStrategy));
        self.register(Box::new(HeaderManipulationStrategy));
        self.register(Box::new(RequestValidationStrategy::new(app_config.clone())));
    }
    
    /// Register a new filter strategy
    pub fn register(&mut self, strategy: Box<dyn FilterStrategy>) {
        self.strategies.push(strategy);
    }
    
    /// Find a strategy that supports the given filter type
    pub fn get_strategy(&self, filter_type: &str) -> Option<&dyn FilterStrategy> {
        self.strategies
            .iter()
            .find(|strategy| strategy.supports(filter_type))
            .map(|boxed| boxed.as_ref())
    }
    
    /// Get all supported filter types
    pub fn supported_filter_types(&self) -> Vec<&'static str> {
        self.strategies
            .iter()
            .map(|strategy| strategy.filter_type())
            .collect()
    }
    
    /// Validate a filter using the appropriate strategy
    pub fn validate_filter(&self, filter: &InternalHttpFilter) -> Result<(), ConversionError> {
        match self.get_strategy(&filter.filter_type) {
            Some(strategy) => strategy.validate(filter),
            None => Err(ConversionError::UnsupportedFilterType {
                filter_type: filter.filter_type.clone(),
                supported_types: self.supported_filter_types().into_iter().map(String::from).collect(),
            })
        }
    }
    
    /// Convert a filter using the appropriate strategy
    pub fn convert_filter(&self, filter: &InternalHttpFilter) -> Result<ConfigType, ConversionError> {
        tracing::info!("🔍 FilterRegistry: Converting filter '{}' of type '{}'", filter.name, filter.filter_type);
        
        match self.get_strategy(&filter.filter_type) {
            Some(strategy) => {
                tracing::info!("📌 FilterRegistry: Found strategy for '{}' filter", filter.filter_type);
                
                // First validate, then convert
                tracing::info!("🔍 FilterRegistry: Validating filter '{}'", filter.name);
                strategy.validate(filter)?;
                tracing::info!("✅ FilterRegistry: Validation passed for filter '{}'", filter.name);
                
                tracing::info!("🔄 FilterRegistry: Converting filter '{}'", filter.name);
                let result = strategy.convert(filter);
                match result {
                    Ok(_) => tracing::info!("✅ FilterRegistry: Successfully converted filter '{}'", filter.name),
                    Err(ref e) => tracing::error!("❌ FilterRegistry: Failed to convert filter '{}': {}", filter.name, e),
                }
                result
            }
            None => {
                tracing::error!("❌ FilterRegistry: No strategy found for filter type '{}'", filter.filter_type);
                tracing::info!("📋 FilterRegistry: Supported types: {:?}", self.supported_filter_types());
                Err(ConversionError::UnsupportedFilterType {
                    filter_type: filter.filter_type.clone(),
                    supported_types: self.supported_filter_types().into_iter().map(String::from).collect(),
                })
            }
        }
    }
}

// Note: Default implementation is not provided because FilterStrategyRegistry
// requires an AppConfig to properly configure strategies like RequestValidationStrategy
// Users should call FilterStrategyRegistry::new(app_config) explicitly