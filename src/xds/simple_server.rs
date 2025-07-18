use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};

use crate::storage::ConfigStore;
use crate::xds::conversion::ProtoConverter;

// Include the generated protobuf code
include!(concat!(env!("OUT_DIR"), "/envoy.service.discovery.v3.rs"));

pub use aggregated_discovery_service_server::AggregatedDiscoveryServiceServer;

#[derive(Debug, Clone)]
pub struct SimpleXdsServer {
    store: ConfigStore,
    nonce_counter: Arc<AtomicU64>,
    version_counter: Arc<AtomicU64>,
    update_sender: broadcast::Sender<()>,
}

impl SimpleXdsServer {
    pub fn new(store: ConfigStore) -> Self {
        let (update_sender, _) = broadcast::channel(100);
        Self {
            store,
            nonce_counter: Arc::new(AtomicU64::new(0)),
            version_counter: Arc::new(AtomicU64::new(1)),
            update_sender,
        }
    }

    fn generate_nonce(&self) -> String {
        // Use simple incrementing integers like Go control plane
        let nonce = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        nonce.to_string()
    }

    /// Increment version when resources change
    /// This should be called whenever resources are added/updated/deleted
    pub fn increment_version(&self) {
        let new_version = self.version_counter.fetch_add(1, Ordering::SeqCst) + 1;
        println!("📈 Version incremented to: {}", new_version);
        
        // Notify all connected Envoy instances of the update
        // We ignore the error if no receivers are listening
        let _ = self.update_sender.send(());
        println!("📢 Broadcast update notification sent to all connected Envoy instances");
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
        let version_counter = self.version_counter.clone();
        let store = self.store.clone();
        let mut update_receiver = self.update_sender.subscribe();

        tokio::spawn(async move {
            let mut last_sent_version = 0;
            let mut pending_types: Vec<String> = Vec::new();
            
            loop {
                tokio::select! {
                    // Handle incoming requests from Envoy
                    message = stream.message() => {
                        match message {
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
                                
                                // Track what type this client is interested in
                                if !pending_types.contains(&request.type_url) {
                                    pending_types.push(request.type_url.clone());
                                }
                                
                                println!("📨 ADS: This is an initial request, sending response");
                                
                                // Get actual resources from the store using the conversion module
                                let resources = match ProtoConverter::get_resources_by_type(&request.type_url, &store) {
                                    Ok(resources) => {
                                        println!("✅ ADS: Found {} resources for type: {}", resources.len(), request.type_url);
                                        resources
                                    }
                                    Err(e) => {
                                        println!("❌ ADS: Error getting resources for type {}: {}", request.type_url, e);
                                        vec![]
                                    }
                                };
                                
                                let response_nonce = nonce_counter.fetch_add(1, Ordering::SeqCst).to_string();
                                let current_version = version_counter.load(Ordering::SeqCst);
                                last_sent_version = current_version;
                                
                                let response = DiscoveryResponse {
                                    version_info: current_version.to_string(),
                                    resources,
                                    canary: false,
                                    type_url: request.type_url.clone(),
                                    nonce: response_nonce.clone(),
                                };
                                
                                println!("📤 ADS: Sending response for type: {}, nonce: {}, version: {}", request.type_url, response_nonce, current_version);
                                
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
                    
                    // Handle update notifications (when resources change)
                    _ = update_receiver.recv() => {
                        let current_version = version_counter.load(Ordering::SeqCst);
                        
                        // Only send updates if version has changed and we have types to update
                        if current_version > last_sent_version && !pending_types.is_empty() {
                            println!("🔄 ADS: Pushing resource updates for version: {}", current_version);
                            
                            // Send updates for all types this client is interested in
                            for type_url in &pending_types {
                                let resources = match ProtoConverter::get_resources_by_type(type_url, &store) {
                                    Ok(resources) => {
                                        println!("✅ ADS: Found {} resources for type: {}", resources.len(), type_url);
                                        resources
                                    }
                                    Err(e) => {
                                        println!("❌ ADS: Error getting resources for type {}: {}", type_url, e);
                                        vec![]
                                    }
                                };
                                
                                let response_nonce = nonce_counter.fetch_add(1, Ordering::SeqCst).to_string();
                                let response = DiscoveryResponse {
                                    version_info: current_version.to_string(),
                                    resources,
                                    canary: false,
                                    type_url: type_url.clone(),
                                    nonce: response_nonce.clone(),
                                };
                                
                                println!("📤 ADS: Pushing update for type: {}, nonce: {}, version: {}", type_url, response_nonce, current_version);
                                
                                if tx.send(Ok(response)).await.is_err() {
                                    println!("❌ ADS: Failed to send push update");
                                    break;
                                }
                            }
                            
                            last_sent_version = current_version;
                            println!("✅ ADS: All push updates sent successfully");
                        }
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}