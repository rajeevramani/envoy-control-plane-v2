use std::pin::Pin;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::storage::ConfigStore;
use super::conversion::ProtoConverter;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

// Re-export the server types for main.rs
pub use aggregated_discovery_service_server::AggregatedDiscoveryServiceServer;
pub use route_discovery_service_server::RouteDiscoveryServiceServer;
pub use cluster_discovery_service_server::ClusterDiscoveryServiceServer;

#[derive(Debug, Clone)]
pub struct XdsServer {
    store: ConfigStore,
    version_counter: Arc<RwLock<u64>>,
}

impl XdsServer {
    pub fn new(store: ConfigStore) -> Self {
        Self {
            store,
            version_counter: Arc::new(RwLock::new(0)),
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
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            let mut client_versions: HashMap<String, String> = HashMap::new();
            
            while let Some(request) = stream.message().await.unwrap_or(None) {
                let type_url = request.type_url.as_str();
                let version = {
                    let mut counter = version_counter.write().await;
                    *counter += 1;
                    counter.to_string()
                };

                let response = match type_url {
                    "type.googleapis.com/envoy.config.route.v3.RouteConfiguration" => {
                        let routes = store.list_routes();
                        match ProtoConverter::routes_to_proto(routes) {
                            Ok(proto_routes) => Some(DiscoveryResponse {
                                version_info: version.clone(),
                                resources: proto_routes,
                                type_url: type_url.to_string(),
                                nonce: Self::generate_nonce(),
                            }),
                            Err(_) => None,
                        }
                    }
                    "type.googleapis.com/envoy.config.cluster.v3.Cluster" => {
                        let clusters = store.list_clusters();
                        match ProtoConverter::clusters_to_proto(clusters) {
                            Ok(proto_clusters) => Some(DiscoveryResponse {
                                version_info: version.clone(),
                                resources: proto_clusters,
                                type_url: type_url.to_string(),
                                nonce: Self::generate_nonce(),
                            }),
                            Err(_) => None,
                        }
                    }
                    _ => None,
                };

                if let Some(response) = response {
                    client_versions.insert(type_url.to_string(), version);
                    if tx.send(Ok(response)).await.is_err() {
                        break;
                    }
                }
            }
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
        println!("🔗 RDS: Connection established, starting stream");
        let mut stream = request.into_inner();
        let store = self.store.clone();
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(request) = stream.message().await.unwrap_or(None) {
                println!("🔄 RDS: Received request for routes, type_url: {}", request.type_url);
                let routes = store.list_routes();
                println!("📋 RDS: Found {} routes", routes.len());
                match ProtoConverter::routes_to_proto(routes) {
                    Ok(proto_routes) => {
                        let response = DiscoveryResponse {
                            version_info: "1".to_string(),
                            resources: proto_routes,
                            type_url: "type.googleapis.com/envoy.config.route.v3.RouteConfiguration".to_string(),
                            nonce: Self::generate_nonce(),
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
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
        println!("🔗 CDS: Connection established, starting stream");
        let mut stream = request.into_inner();
        let store = self.store.clone();
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(request) = stream.message().await.unwrap_or(None) {
                println!("🔄 CDS: Received request for clusters, type_url: {}", request.type_url);
                let clusters = store.list_clusters();
                println!("📋 CDS: Found {} clusters", clusters.len());
                match ProtoConverter::clusters_to_proto(clusters) {
                    Ok(proto_clusters) => {
                        let response = DiscoveryResponse {
                            version_info: "1".to_string(),
                            resources: proto_clusters,
                            type_url: "type.googleapis.com/envoy.config.cluster.v3.Cluster".to_string(),
                            nonce: Self::generate_nonce(),
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}