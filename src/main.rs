mod config;
mod storage;
mod api;
mod envoy;

use config::AppConfig;
use storage::ConfigStore;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Load configuration and create storage
    let config = AppConfig::load()?;
    let store = ConfigStore::new();
    
    // Create API router
    let app = api::create_router(store);
    
    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.rest_port);
    println!("Envoy Control Plane starting...");
    println!("REST API running on http://{}", addr);
    
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}
