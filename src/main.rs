mod config;
mod storage;
mod api;
mod envoy;
mod xds;

use config::AppConfig;
use storage::ConfigStore;
use tokio::net::TcpListener;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Load configuration and create storage
    let config = AppConfig::load()?;
    let store = ConfigStore::new();
    
    // Create API router
    let app = api::create_router(store.clone());
    
    // Create xDS server
    let xds_server = xds::SimpleXdsServer::new(store.clone());
    
    // Start both servers concurrently
    let rest_addr = format!("{}:{}", config.server.host, config.server.rest_port);
    let xds_addr = format!("{}:{}", config.server.host, config.server.xds_port);
    
    println!("Envoy Control Plane starting...");
    println!("REST API running on http://{}", rest_addr);
    println!("xDS gRPC server running on http://{}", xds_addr);
    
    // Start REST server
    let rest_listener = TcpListener::bind(&rest_addr).await?;
    let rest_server = axum::serve(rest_listener, app);
    
    // Start xDS gRPC server  
    let xds_server_addr = xds_addr.parse()?;
    
    println!("🔧 Registering gRPC services:");
    println!("  - AggregatedDiscoveryService (ADS)");
    
    let xds_service = Server::builder()
        .add_service(xds::AggregatedDiscoveryServiceServer::new(xds_server))
        .serve(xds_server_addr);
    
    // Run both servers concurrently
    tokio::select! {
        result = rest_server => {
            if let Err(e) = result {
                eprintln!("REST server error: {}", e);
            }
        }
        result = xds_service => {
            if let Err(e) = result {
                eprintln!("xDS server error: {}", e);
            }
        }
    }
    
    Ok(())
}
