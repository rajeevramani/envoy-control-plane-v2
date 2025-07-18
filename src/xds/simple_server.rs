use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};

use crate::storage::ConfigStore;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

pub use aggregated_discovery_service_server::AggregatedDiscoveryServiceServer;

#[derive(Debug, Clone)]
pub struct SimpleXdsServer {
    store: ConfigStore,
    nonce_counter: Arc<AtomicU64>,
}

impl SimpleXdsServer {
    pub fn new(store: ConfigStore) -> Self {
        Self {
            store,
            nonce_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    fn generate_nonce(&self) -> String {
        // Use simple incrementing integers like Go control plane
        let nonce = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        nonce.to_string()
    }
}

#[tonic::async_trait]
impl aggregated_discovery_service_server::AggregatedDiscoveryService for SimpleXdsServer {
    type StreamAggregatedResourcesStream = Pin<Box<dyn Stream<Item = Result<DiscoveryResponse, Status>> + Send>>;

    async fn stream_aggregated_resources(
        &self,
        request: Request<Streaming<DiscoveryRequest>>,
    ) -> Result<Response<Self::StreamAggregatedResourcesStream>, Status> {
        println!("🔗 ADS: Connection established, starting stream");
        
        let mut stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let nonce_counter = self.nonce_counter.clone();

        tokio::spawn(async move {
            loop {
                match stream.message().await {
                    Ok(Some(request)) => {
                        println!("🔄 ADS: Received request for type: {}", request.type_url);
                        println!("🔄 ADS: Version: '{}', Nonce: '{}'", request.version_info, request.nonce);
                        println!("🔄 ADS: Resource names: {:?}", request.resource_names);
                        
                        // Check if this is an ACK/NACK (has our previous nonce) or initial request
                        let is_ack_or_nack = !request.nonce.is_empty();
                        
                        if is_ack_or_nack {
                            if let Some(error_detail) = &request.error_detail {
                                println!("❌ ADS: This is a NACK for nonce: {} - Error: {}", request.nonce, error_detail.message);
                                // Handle NACK - could resend previous version or fix config
                            } else {
                                println!("✅ ADS: This is an ACK for nonce: {} (client accepted our config)", request.nonce);
                                // ACK received - client accepted our config
                            }
                            // For ACKs/NACKs, we don't need to send another response unless config changed
                            // Just continue listening for more requests
                            continue;
                        }
                        
                        println!("📨 ADS: This is an initial request, sending response");
                        
                        // Respond to requests with empty resources (valid for any type)
                        let response_nonce = nonce_counter.fetch_add(1, Ordering::SeqCst).to_string();
                        let response = DiscoveryResponse {
                            version_info: "1".to_string(),
                            resources: vec![], // Empty resources - this is valid
                            canary: false,
                            type_url: request.type_url.clone(),
                            nonce: response_nonce.clone(),
                        };
                        
                        println!("📤 ADS: Sending response for type: {}, nonce: {}", request.type_url, response_nonce);
                        
                        if tx.send(Ok(response)).await.is_err() {
                            println!("❌ ADS: Failed to send response");
                            break;
                        }
                        
                        println!("✅ ADS: Response sent successfully, waiting for next request...");
                    }
                    Ok(None) => {
                        println!("🔚 ADS: Client closed stream");
                        break;
                    }
                    Err(e) => {
                        println!("❌ ADS: Stream error: {}", e);
                        // Don't break on error, just continue
                        continue;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}