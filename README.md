# Envoy Control Plane (Experimental)

An **experimental Envoy control plane** implementation in Rust that provides dynamic configuration management for Envoy proxies via the xDS protocol. This is a learning project and **not production-ready**.

## 🏗️ Architecture Overview

This control plane implements a **dual-server architecture** with real-time configuration updates:

```
┌─────────────┐    HTTP     ┌─────────────┐    Store    ┌─────────────┐
│   Client    │────────────▶│  REST API   │────────────▶│   Storage   │
│  (curl/UI)  │             │   (axum)    │             │ (DashMap)   │
└─────────────┘             └─────────────┘             └─────────────┘
                                    │                           │
                                    ▼                           │
                            increment_version()                 │
                                    │                           │
                                    ▼                           │
┌─────────────┐   gRPC/xDS  ┌─────────────┐    Read     ┌─────┴─────────┐
│    Envoy    │◀────────────│ xDS Server  │◀────────────│   Storage     │
│   Proxy     │             │  (tonic)    │             │  (DashMap)    │
└─────────────┘             └─────────────┘             └───────────────┘
```

### Key Features

- ✅ **RESTful API** for route and cluster management
- ✅ **Real-time updates** to Envoy via xDS protocol (ADS)
- ✅ **Thread-safe storage** with lock-free concurrent access
- ✅ **Type-safe protobuf** integration with envoy-types
- ✅ **ACK/NACK handling** for reliable configuration delivery
- ✅ **Prefix rewriting** for URL transformation
- ✅ **Load balancing** configuration per cluster
- ✅ **Configuration versioning** and change tracking

### ⚠️ Experimental Status

This control plane is **experimental** and not suitable for production use. Known limitations:

- **No persistence** - Configuration lost on restart
- **No authentication/authorization** - Open REST API
- **Limited error handling** - Basic error recovery
- **Hardcoded policies** - Load balancing and timeouts are fixed
- **No metrics/monitoring** - Basic logging only
- **Single instance only** - No high availability

## 🚀 Quick Start

### Prerequisites

- **Rust** 1.70+ ([install](https://rustup.rs/))
- **Envoy** 1.24+ ([download](https://www.envoyproxy.io/docs/envoy/latest/start/install))

### 1. Clone and Build

```bash
git clone <repository-url>
cd envoy-control-plane-v2
cargo build --release
```

### 2. Configure the Control Plane

Edit `config.yaml`:

```yaml
server:
  rest_port: 8080    # REST API port
  xds_port: 18000    # xDS gRPC server port
  host: "0.0.0.0"    # Bind address

envoy:
  config_dir: "./configs"  # Generated config directory
  admin_port: 9901        # Envoy admin port

logging:
  level: "info"      # Log level
```

### 3. Start the Control Plane

```bash
cargo run
```

Expected output:
```
Envoy Control Plane starting...
REST API running on http://0.0.0.0:8080
xDS gRPC server running on http://0.0.0.0:18000
🔧 Registering gRPC services:
  - AggregatedDiscoveryService (ADS)
```

### 4. Start Envoy with Dynamic Configuration

```bash
envoy -c envoy-bootstrap.yaml
```

Expected output:
```
🔗 ADS: Connection established, starting stream
🔄 ADS: Received request for type: type.googleapis.com/envoy.config.cluster.v3.Cluster
📤 ADS: Sending response for type: type.googleapis.com/envoy.config.cluster.v3.Cluster
✅ ADS: Response sent successfully
```

## 📡 API Documentation

### Clusters

#### Create Cluster
```bash
curl -X POST http://localhost:8080/clusters \
  -H "Content-Type: application/json" \
  -d '{
    "name": "backend-service",
    "endpoints": [
      {"host": "10.0.1.10", "port": 8080},
      {"host": "10.0.1.11", "port": 8080}
    ]
  }'
```

#### List Clusters
```bash
curl http://localhost:8080/clusters
```

#### Get Specific Cluster
```bash
curl http://localhost:8080/clusters/backend-service
```

#### Delete Cluster
```bash
curl -X DELETE http://localhost:8080/clusters/backend-service
```

### Routes

#### Create Route
```bash
curl -X POST http://localhost:8080/routes \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/api/users",
    "cluster_name": "backend-service",
    "prefix_rewrite": "/v2/users"
  }'
```

**Route Fields:**
- `path`: URL prefix to match (e.g., "/api/users")
- `cluster_name`: Target cluster for matched requests
- `prefix_rewrite`: (Optional) URL transformation

#### List Routes
```bash
curl http://localhost:8080/routes
```

#### Get Specific Route
```bash
curl http://localhost:8080/routes/{route-id}
```

#### Delete Route
```bash
curl -X DELETE http://localhost:8080/routes/{route-id}
```

### Health Check
```bash
curl http://localhost:8080/health
```

## 🔄 How It Works

### 1. Configuration Flow

1. **Create cluster** via REST API
2. **Create routes** pointing to that cluster
3. **Control plane** stores configuration in thread-safe storage
4. **Version counter** increments automatically
5. **xDS server** pushes updates to connected Envoy instances
6. **Envoy** applies new configuration immediately

### 2. Real-time Updates

```bash
# Add a new route
curl -X POST http://localhost:8080/routes -d '{
  "path": "/new-api",
  "cluster_name": "backend-service"
}'

# Immediately available through Envoy!
curl http://localhost:10000/new-api
```

### 3. URL Rewriting

Create a route with prefix rewriting:

```bash
curl -X POST http://localhost:8080/routes -d '{
  "path": "/api/v1",
  "cluster_name": "backend-service", 
  "prefix_rewrite": "/v2"
}'
```

**Result:**
- **Client request**: `GET /api/v1/users`
- **Envoy forwards**: `GET /v2/users` to backend-service
- **Seamless API versioning!**

## 🏛️ Project Structure

```
src/
├── main.rs              # Application bootstrap and server coordination
├── config/              # Configuration management
│   └── mod.rs           # YAML config loading with serde
├── storage/             # Thread-safe data storage
│   ├── mod.rs           # Public API and re-exports
│   ├── models.rs        # Route, Cluster, Endpoint structs
│   └── store.rs         # DashMap-based concurrent storage
├── api/                 # REST API layer
│   ├── mod.rs           # Router creation and state management
│   ├── handlers.rs      # HTTP request handlers
│   └── routes.rs        # Route definitions and AppState
├── xds/                 # xDS protocol implementation
│   ├── mod.rs           # xDS module exports
│   ├── simple_server.rs # gRPC server and streaming logic
│   └── conversion.rs    # Protobuf conversion (Rust ↔ Envoy)
└── envoy/              # Static Envoy config generation
    └── mod.rs          # Legacy config generator
```

## ⚙️ Configuration

### Control Plane Configuration (`config.yaml`)

```yaml
server:
  rest_port: 8080       # REST API port
  xds_port: 18000       # xDS gRPC server port  
  host: "0.0.0.0"       # Bind address

envoy:
  config_dir: "./configs"   # Directory for generated configs
  admin_port: 9901         # Envoy admin interface port

logging:
  level: "info"         # debug, info, warn, error
```

### Envoy Bootstrap Configuration (`envoy-bootstrap.yaml`)

The bootstrap configures Envoy to connect to our control plane:

```yaml
# Dynamic resources from our control plane
dynamic_resources:
  ads_config:
    api_type: GRPC
    transport_api_version: V3
    grpc_services:
      - envoy_grpc:
          cluster_name: control_plane_cluster

  cds_config:             # Cluster Discovery Service
    ads: {}
    resource_api_version: V3

# Static configuration  
static_resources:
  listeners:
  - name: listener_0
    address:
      socket_address:
        address: 0.0.0.0
        port_value: 10000   # Envoy listens here for traffic
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          rds:                # Route Discovery Service
            config_source:
              ads: {}
              resource_api_version: V3
            route_config_name: local_route
          http_filters:
          - name: envoy.filters.http.router

  clusters:
  - name: control_plane_cluster    # How to reach our control plane
    type: STRICT_DNS
    lb_policy: ROUND_ROBIN
    http2_protocol_options: {}     # gRPC requires HTTP/2
    load_assignment:
      cluster_name: control_plane_cluster
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address:
                address: 127.0.0.1  # Control plane address
                port_value: 18000   # Control plane xDS port
```

## 🔍 Monitoring and Debugging

### Control Plane Logs

The control plane provides detailed logging:

```
📈 Version incremented to: 6
📢 Broadcast update notification sent to all connected Envoy instances
🔄 ADS: Pushing resource updates for version: 6
✅ Routes conversion: Creating RouteConfiguration with 2 routes
📤 ADS: Pushing update for type: type.googleapis.com/envoy.config.route.v3.RouteConfiguration
✅ ADS: All push updates sent successfully
```

### Envoy Admin Interface

Access Envoy's admin interface at `http://localhost:9901`:

- **`/config_dump`** - Current configuration
- **`/clusters`** - Cluster status and health
- **`/stats`** - Detailed metrics
- **`/listeners`** - Listener configuration

### Health Checks

```bash
# Control plane health
curl http://localhost:8080/health

# Envoy admin health  
curl http://localhost:9901/ready
```

## 🧪 Testing

### Run Tests
```bash
cargo test
```

### Test with Real Traffic

1. **Start control plane and Envoy** (see Quick Start)

2. **Create a test cluster:**
```bash
curl -X POST http://localhost:8080/clusters -d '{
  "name": "httpbin-service",
  "endpoints": [{"host": "httpbin.org", "port": 80}]
}'
```

3. **Create a test route:**
```bash
curl -X POST http://localhost:8080/routes -d '{
  "path": "/test",
  "cluster_name": "httpbin-service",
  "prefix_rewrite": "/get"
}'
```

4. **Test the routing:**
```bash
curl http://localhost:10000/test
# Should return httpbin.org/get response
```

## 🚨 Troubleshooting

### Common Issues

**1. "Connection refused" on startup**
- Check if ports 8080 and 18000 are available
- Verify firewall settings
- Check `config.yaml` host/port settings

**2. "Envoy can't connect to control plane"**
- Verify `envoy-bootstrap.yaml` has correct control plane address
- Check control plane is running: `curl http://localhost:8080/health`
- Look for connection logs in control plane output

**3. "Route not working"**
- Ensure cluster exists before creating routes
- Check Envoy logs for NACK messages
- Verify route path matches your test URL

**4. "Configuration not updating"**
- Check for NACK errors in control plane logs
- Verify Envoy admin interface shows updated config: `http://localhost:9901/config_dump`
- Ensure route points to existing cluster

### Debug Mode

Enable debug logging in `config.yaml`:
```yaml
logging:
  level: "debug"
```

## 🛠️ Development

### Key Dependencies

- **`axum`** - Modern async web framework for REST API
- **`tonic`** - gRPC framework for xDS implementation  
- **`tokio`** - Async runtime for concurrent server execution
- **`dashmap`** - Lock-free concurrent HashMap for storage
- **`envoy-types`** - Type-safe Envoy protobuf definitions
- **`serde`** - Serialization for JSON APIs and config files

### Architecture Patterns

- **Dual-server design** - REST and gRPC servers sharing storage
- **Arc-based sharing** - Thread-safe reference counting for shared state
- **Atomic operations** - Lock-free version counters and nonces
- **Broadcast channels** - One-to-many notifications for real-time updates
- **Type-safe protobuf** - Compile-time verification of xDS messages

## 📚 Learning Resources

- [Envoy xDS Protocol](https://www.envoyproxy.io/docs/envoy/latest/api-docs/xds_protocol)
- [Envoy Configuration Reference](https://www.envoyproxy.io/docs/envoy/latest/configuration/configuration)
- [gRPC and Tonic Guide](https://github.com/hyperium/tonic)
- [Rust Async Programming](https://rust-lang.github.io/async-book/)

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality  
4. Ensure all tests pass: `cargo test`
5. Submit a pull request

## 📄 License

[Add your license here]