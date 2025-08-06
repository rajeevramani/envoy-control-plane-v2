use std::pin::Pin;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn, debug};
use uuid::Uuid;

use crate::storage::ConfigStore;
use super::conversion::ProtoConverter;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

// Import Status for NACK responses
use envoy_types::pb::google::rpc::Status;

/// Circuit breaker for protecting against cascading failures
#[derive(Debug, Clone)]
struct CircuitBreaker {
    failure_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            failure_threshold,
            recovery_timeout,
        }
    }

    async fn is_circuit_open(&self) -> bool {
        let failure_count = *self.failure_count.read().await;
        if failure_count >= self.failure_threshold {
            let last_failure = *self.last_failure_time.read().await;
            if let Some(last_failure_time) = last_failure {
                return last_failure_time.elapsed() < self.recovery_timeout;
            }
        }
        false
    }

    async fn record_failure(&self) {
        let mut failure_count = self.failure_count.write().await;
        let mut last_failure_time = self.last_failure_time.write().await;
        *failure_count += 1;
        *last_failure_time = Some(Instant::now());
        warn!("Circuit breaker: failure count increased to {}", *failure_count);
    }

    async fn record_success(&self) {
        let mut failure_count = self.failure_count.write().await;
        let mut last_failure_time = self.last_failure_time.write().await;
        *failure_count = 0;
        *last_failure_time = None;
        debug!("Circuit breaker: success recorded, failure count reset");
    }
}

// Re-export the server types for main.rs
pub use aggregated_discovery_service_server::AggregatedDiscoveryServiceServer;
pub use route_discovery_service_server::RouteDiscoveryServiceServer;
pub use cluster_discovery_service_server::ClusterDiscoveryServiceServer;

#[derive(Debug, Clone)]
pub struct XdsServer {
    store: ConfigStore,
    version_counter: Arc<RwLock<u64>>,
    circuit_breaker: CircuitBreaker,
}

impl XdsServer {
    pub fn new(store: ConfigStore) -> Self {
        Self {
            store,
            version_counter: Arc::new(RwLock::new(0)),
            circuit_breaker: CircuitBreaker::new(
                5, // failure threshold
                Duration::from_secs(30) // recovery timeout
            ),
        }
    }

    async fn increment_version(&self) -> String {
        let mut counter = self.version_counter.write().await;
        *counter += 1;
        counter.to_string()
    }

    fn generate_nonce() -> String {
        Uuid::new_v4().to_string()
    }

    /// Create XDS protocol compliant error response with enhanced context
    fn create_error_response(error: &tonic::Status, context: &str) -> tonic::Status {
        let error_msg = match error.code() {
            tonic::Code::DeadlineExceeded => {
                format!("XDS stream timeout exceeded in {}", context)
            }
            tonic::Code::Cancelled => {
                format!("XDS client cancelled request during {}", context)
            }
            tonic::Code::Unavailable => {
                format!("XDS service temporarily unavailable for {}", context)
            }
            tonic::Code::ResourceExhausted => {
                format!("XDS service resource exhausted during {}", context)
            }
            _ => {
                format!("XDS stream processing error in {}: {}", context, error.message())
            }
        };

        // Log the error for operational visibility
        error!("XDS Error: {}", error_msg);
        
        // Return appropriate status maintaining xDS protocol compliance
        match error.code() {
            tonic::Code::DeadlineExceeded => Status::deadline_exceeded(error_msg),
            tonic::Code::Cancelled => Status::cancelled(error_msg),
            tonic::Code::Unavailable => Status::unavailable(error_msg),
            tonic::Code::ResourceExhausted => Status::resource_exhausted(error_msg),
            _ => Status::internal(error_msg)
        }
    }

    /// Create error response for configuration conversion failures with recovery guidance
    fn create_config_error_response(resource_type: &str, error: &anyhow::Error, resource_count: usize) -> tonic::Status {
        let error_msg = format!(
            "Failed to generate {} configuration (affecting {} resources): {}. This may indicate invalid configuration data or temporary conversion issues.",
            resource_type, resource_count, error
        );
        
        // Log detailed error for debugging
        error!("Configuration conversion error - Type: {}, Count: {}, Error: {}", 
               resource_type, resource_count, error);
               
        // For conversion errors, use INTERNAL status as per xDS protocol
        Status::internal(error_msg)
    }

    /// Create NACK response for invalid client requests (xDS protocol compliance)
    fn create_nack_response(type_url: &str, nonce: &str, error_detail: &str) -> DiscoveryResponse {
        warn!("Sending NACK response for {}: {}", type_url, error_detail);
        
        DiscoveryResponse {
            version_info: "".to_string(), // Empty version indicates NACK
            resources: vec![],
            type_url: type_url.to_string(),
            nonce: nonce.to_string(),
            error_detail: Some(Status {
                code: tonic::Code::InvalidArgument as i32,
                message: error_detail.to_string(),
                details: vec![],
            }),
        }
    }

    /// Safely process resources with circuit breaker protection
    async fn process_routes_safely(&self, version: String) -> Result<DiscoveryResponse, tonic::Status> {
        // Check circuit breaker state
        if self.circuit_breaker.is_circuit_open().await {
            let error_msg = "Circuit breaker is open - service temporarily degraded";
            warn!("Route processing blocked by circuit breaker");
            return Err(Status::unavailable(error_msg));
        }

        // Attempt to process routes
        let routes = self.store.list_routes();
        match ProtoConverter::routes_to_proto(routes) {
            Ok(proto_routes) => {
                let response = DiscoveryResponse {
                    version_info: version,
                    resources: proto_routes,
                    type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
                    nonce: Self::generate_nonce(),
                    error_detail: None,
                };
                
                // Record success in circuit breaker
                self.circuit_breaker.record_success().await;
                Ok(response)
            }
            Err(e) => {
                // Record failure in circuit breaker
                self.circuit_breaker.record_failure().await;
                let routes_count = self.store.list_routes().len();
                Err(Self::create_config_error_response("RouteConfiguration", &e, routes_count))
            }
        }
    }

    /// Safely process clusters with circuit breaker protection
    async fn process_clusters_safely(&self, version: String) -> Result<DiscoveryResponse, tonic::Status> {
        // Check circuit breaker state
        if self.circuit_breaker.is_circuit_open().await {
            let error_msg = "Circuit breaker is open - service temporarily degraded";
            warn!("Cluster processing blocked by circuit breaker");
            return Err(Status::unavailable(error_msg));
        }

        // Attempt to process clusters
        let clusters = self.store.list_clusters();
        match ProtoConverter::clusters_to_proto(clusters) {
            Ok(proto_clusters) => {
                let response = DiscoveryResponse {
                    version_info: version,
                    resources: proto_clusters,
                    type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
                    nonce: Self::generate_nonce(),
                    error_detail: None,
                };
                
                // Record success in circuit breaker
                self.circuit_breaker.record_success().await;
                Ok(response)
            }
            Err(e) => {
                // Record failure in circuit breaker
                self.circuit_breaker.record_failure().await;
                let clusters_count = self.store.list_clusters().len();
                Err(Self::create_config_error_response("Cluster", &e, clusters_count))
            }
        }
    }
}

#[tonic::async_trait]
impl aggregated_discovery_service_server::AggregatedDiscoveryService for XdsServer {
    type StreamAggregatedResourcesStream = Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send>>;

    async fn stream_aggregated_resources(
        &self,
        request: Request<Streaming<DiscoveryRequest>>,
    ) -> Result<Response<Self::StreamAggregatedResourcesStream>, Status> {
        let mut stream = request.into_inner();
        let store = self.store.clone();
        let version_counter = self.version_counter.clone();
        let server = self.clone();
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let mut client_versions: HashMap<String, String> = HashMap::new();
            info!("ADS: Starting aggregated discovery stream");
            
            loop {
                match stream.message().await {
                    Ok(Some(request)) => {
                        let type_url = request.type_url.as_str();
                        info!("ADS: Processing request for type: {}", type_url);
                        
                        let version = {
                            let mut counter = version_counter.write().await;
                            *counter += 1;
                            counter.to_string()
                        };

                        match type_url {
                            "type.googleapis.com/envoy.config.route.v3.RouteConfiguration" => {
                                match server.process_routes_safely(version.clone()).await {
                                    Ok(response) => {
                                        client_versions.insert(type_url.to_string(), version);
                                        if let Err(e) = tx.send(Ok(response)).await {
                                            error!("ADS: Failed to send route response: {}", e);
                                            break;
                                        }
                                    }
                                    Err(error_response) => {
                                        if let Err(send_err) = tx.send(Err(error_response)).await {
                                            error!("ADS: Failed to send route error response: {}", send_err);
                                            break;
                                        }
                                    }
                                }
                            }
                            "type.googleapis.com/envoy.config.cluster.v3.Cluster" => {
                                match server.process_clusters_safely(version.clone()).await {
                                    Ok(response) => {
                                        client_versions.insert(type_url.to_string(), version);
                                        if let Err(e) = tx.send(Ok(response)).await {
                                            error!("ADS: Failed to send cluster response: {}", e);
                                            break;
                                        }
                                    }
                                    Err(error_response) => {
                                        if let Err(send_err) = tx.send(Err(error_response)).await {
                                            error!("ADS: Failed to send cluster error response: {}", send_err);
                                            break;
                                        }
                                    }
                                }
                            }
                            _ => {
                                warn!("ADS: Unsupported resource type requested: {}", type_url);
                                let error_response = Status::unimplemented(format!("Resource type {} not supported", type_url));
                                if let Err(e) = tx.send(Err(error_response)).await {
                                    error!("ADS: Failed to send unsupported type error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        info!("ADS: Client disconnected gracefully");
                        break;
                    }
                    Err(e) => {
                        let error_response = Self::create_error_response(&e, "ADS stream processing");
                        if let Err(send_err) = tx.send(Err(error_response)).await {
                            warn!("ADS: Failed to send stream error response: {}", send_err);
                        }
                        break;
                    }
                }
            }
            info!("ADS: Stream processing completed");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[tonic::async_trait]
impl route_discovery_service_server::RouteDiscoveryService for XdsServer {
    type StreamRoutesStream = Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send>>;

    async fn stream_routes(
        &self,
        request: Request<Streaming<DiscoveryRequest>>,
    ) -> Result<Response<Self::StreamRoutesStream>, Status> {
        info!("RDS: Connection established, starting stream");
        let mut stream = request.into_inner();
        let store = self.store.clone();
        let server = self.clone();
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            info!("RDS: Starting route discovery stream");
            
            loop {
                match stream.message().await {
                    Ok(Some(request)) => {
                        info!("RDS: Received request for routes, type_url: {}", request.type_url);
                        let routes_count = store.list_routes().len();
                        info!("RDS: Found {} routes to serve", routes_count);
                        
                        match server.process_routes_safely("1".to_string()).await {
                            Ok(response) => {
                                if let Err(e) = tx.send(Ok(response)).await {
                                    error!("RDS: Failed to send route response: {}", e);
                                    break;
                                }
                            }
                            Err(error_response) => {
                                if let Err(send_err) = tx.send(Err(error_response)).await {
                                    error!("RDS: Failed to send route error response: {}", send_err);
                                }
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        info!("RDS: Client disconnected gracefully");
                        break;
                    }
                    Err(e) => {
                        let error_response = Self::create_error_response(&e, "RDS stream processing");
                        if let Err(send_err) = tx.send(Err(error_response)).await {
                            warn!("RDS: Failed to send stream error response: {}", send_err);
                        }
                        break;
                    }
                }
            }
            info!("RDS: Stream processing completed");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[tonic::async_trait]
impl cluster_discovery_service_server::ClusterDiscoveryService for XdsServer {
    type StreamClustersStream = Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send>>;

    async fn stream_clusters(
        &self,
        request: Request<Streaming<DiscoveryRequest>>,
    ) -> Result<Response<Self::StreamClustersStream>, Status> {
        info!("CDS: Connection established, starting stream");
        let mut stream = request.into_inner();
        let store = self.store.clone();
        let server = self.clone();
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            info!("CDS: Starting cluster discovery stream");
            
            loop {
                match stream.message().await {
                    Ok(Some(request)) => {
                        info!("CDS: Received request for clusters, type_url: {}", request.type_url);
                        let clusters_count = store.list_clusters().len();
                        info!("CDS: Found {} clusters to serve", clusters_count);
                        
                        match server.process_clusters_safely("1".to_string()).await {
                            Ok(response) => {
                                if let Err(e) = tx.send(Ok(response)).await {
                                    error!("CDS: Failed to send cluster response: {}", e);
                                    break;
                                }
                            }
                            Err(error_response) => {
                                if let Err(send_err) = tx.send(Err(error_response)).await {
                                    error!("CDS: Failed to send cluster error response: {}", send_err);
                                }
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        info!("CDS: Client disconnected gracefully");
                        break;
                    }
                    Err(e) => {
                        let error_response = Self::create_error_response(&e, "CDS stream processing");
                        if let Err(send_err) = tx.send(Err(error_response)).await {
                            warn!("CDS: Failed to send stream error response: {}", send_err);
                        }
                        break;
                    }
                }
            }
            info!("CDS: Stream processing completed");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}