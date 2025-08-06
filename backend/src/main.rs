mod api;
mod auth;
mod auth_handlers;
mod auth_middleware;
mod config;
mod envoy;
mod rbac;
mod security;
mod storage;
mod validation;
mod xds;

use auth::JwtKeys;
use config::AppConfig;
use rbac::RbacEnforcer;
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

    // Create xDS server
    let xds_server = xds::SimpleXdsServer::new(store.clone());

    // Initialize authentication components
    let jwt_keys = JwtKeys::new(config.control_plane.authentication.clone());
    
    // Initialize RBAC (create simple in-memory enforcer for now)
    let rbac = RbacEnforcer::new_simple().await?;

    // Create API router with all components
    let app = api::create_router(store.clone(), xds_server.clone(), jwt_keys, rbac);

    // Start both servers concurrently
    let rest_addr = format!(
        "{}:{}",
        config.control_plane.server.host, config.control_plane.server.rest_port
    );
    let xds_addr = format!(
        "{}:{}",
        config.control_plane.server.host, config.control_plane.server.xds_port
    );

    println!("Envoy Control Plane starting...");
    println!("REST API running on http://{rest_addr}");

    if config.control_plane.tls.enabled {
        println!("xDS gRPC server running on https://{xds_addr} (TLS enabled)");
    } else {
        println!("xDS gRPC server running on http://{xds_addr} (TLS disabled)");
    }

    // Start REST server
    let rest_listener = TcpListener::bind(&rest_addr).await?;
    let rest_server = axum::serve(rest_listener, app);

    // Start xDS gRPC server (with optional TLS)
    let xds_server_addr = xds_addr.parse()?;

    println!("🔧 Registering gRPC services:");
    println!("  - AggregatedDiscoveryService (ADS)");

    // Create server with optional TLS based on configuration
    let xds_service = if config.control_plane.tls.enabled {
        println!("🔒 TLS enabled - creating secure gRPC server");

        // Load TLS identity from configuration
        let identity = security::load_tls_identity(&config.control_plane.tls)?;

        // Create TLS-enabled server
        let mut tls_server = security::create_tls_server(identity)?;

        tls_server
            .add_service(xds::AggregatedDiscoveryServiceServer::new(xds_server))
            .serve(xds_server_addr)
    } else {
        println!("🔓 TLS disabled - creating plain gRPC server");

        // Create plain gRPC server
        Server::builder()
            .add_service(xds::AggregatedDiscoveryServiceServer::new(xds_server))
            .serve(xds_server_addr)
    };

    // Run both servers concurrently
    tokio::select! {
        result = rest_server => {
            if let Err(e) = result {
                eprintln!("REST server error: {e}");
            }
        }
        result = xds_service => {
            if let Err(e) = result {
                eprintln!("xDS server error: {e}");
            }
        }
    }

    Ok(())
}
