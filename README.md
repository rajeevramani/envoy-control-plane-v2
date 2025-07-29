# Envoy Control Plane v2

A **production-ready Envoy control plane** implementation in Rust that provides dynamic configuration management for Envoy proxies via the xDS protocol with comprehensive validation and configuration management.

## 🎯 What's New in v2

### Major Improvements
- ✅ **Comprehensive Configuration System** - YAML-based with full validation
- ✅ **Dynamic Bootstrap Generation** - Generate Envoy bootstrap from config
- ✅ **Configuration Validation** - Extensive validation with helpful error messages
- ✅ **E2E Testing** - Complete end-to-end test coverage
- ✅ **Zero Hardcoded Values** - Everything configurable via `config.yaml`
- ✅ **Type Safety** - Proper error handling and trait implementations

### Key Features

#### Core Functionality
- ✅ **RESTful API** for route and cluster management
- ✅ **Real-time xDS updates** to Envoy via ADS protocol
- ✅ **Thread-safe storage** with lock-free concurrent access
- ✅ **Type-safe protobuf** integration with envoy-types
- ✅ **ACK/NACK handling** for reliable configuration delivery
- ✅ **Load balancing** configuration per cluster

#### Configuration & Validation
- ✅ **YAML Configuration** - Centralized `config.yaml` for all settings
- ✅ **Dynamic Bootstrap** - Generate Envoy config from control plane settings
- ✅ **Port Validation** - Range checking, conflict detection, privilege warnings
- ✅ **Host Validation** - IP address format, hostname rules, character validation
- ✅ **Timeout Validation** - Sensible ranges with warnings for edge cases
- ✅ **Fail-fast Validation** - Clear error messages at startup

#### Testing & Quality
- ✅ **Comprehensive Test Suite** - Unit, integration, and E2E tests
- ✅ **Automated E2E Testing** - Full workflow testing with generated configs
- ✅ **Code Quality** - Full clippy compliance and formatting
- ✅ **Configuration Testing** - Validation tests for all scenarios

## 🏗️ Architecture Overview

```
┌─────────────┐              ┌─────────────┐    HTTP     ┌─────────────┐    Store    ┌─────────────┐
│  React UI   │──HTTP(3000)─▶│   Client    │────────────▶│  REST API   │────────────▶│   Storage   │
│ (frontend)  │              │  (curl/UI)  │             │   (axum)    │             │ (DashMap)   │
└─────────────┘              └─────────────┘             └─────────────┘             └─────────────┘
                                                                 │                           │
                                                                 ▼                           │
                                                         increment_version()                 │
                                                                 │                           │
                                                                 ▼                           │
                             ┌─────────────┐   gRPC/xDS  ┌─────────────┐    Read     ┌─────┴─────────┐
                             │    Envoy    │◀────────────│ xDS Server  │◀────────────│   Storage     │
                             │   Proxy     │             │  (tonic)    │             │  (DashMap)    │
                             └─────────────┘             └─────────────┘             └───────────────┘
                                                                 ▲
                                                                 │
                                                         ┌───────────────┐
                                                         │  config.yaml  │──── Dynamic Bootstrap
                                                         │  Validation   │──── Generation
                                                         └───────────────┘
```

## 🚀 Quick Start

### Prerequisites

- **Rust** 1.70+ ([install](https://rustup.rs/))
- **Node.js** 18+ ([install](https://nodejs.org/))
- **Envoy** 1.24+ ([download](https://www.envoyproxy.io/docs/envoy/latest/start/install))

### 1. Clone and Build

```bash
git clone <repository-url>
cd envoy-control-plane-v2
make build
```

### 2. Start the Development Environment

```bash
# Terminal 1: Start the backend control plane
make backend-dev

# Terminal 2: Start the frontend web interface
make frontend-dev
```

Expected output (backend):
```
Envoy Control Plane starting...
REST API running on http://0.0.0.0:8080
xDS gRPC server running on http://0.0.0.0:18000
🔧 Registering gRPC services:
  - AggregatedDiscoveryService (ADS)
```

Expected output (frontend):
```
Local:   http://localhost:3000/
Network: http://192.168.1.100:3000/
```

### 3. Access the Web Interface

Open your browser to [http://localhost:3000](http://localhost:3000) to access the web-based management interface with:

- **Dashboard** - Real-time cluster and route counts
- **Clusters** - Add, edit, and delete cluster configurations
- **Routes** - Manage routing rules with path matching
- **Config** - Configuration management tools

### 4. Start Envoy with Generated Configuration

The control plane automatically generates Envoy bootstrap configuration:

```bash
# Generate bootstrap (automatic on startup)
make e2e-generate-bootstrap

# Start Envoy with generated config
envoy -c backend/tests/e2e/envoy-bootstrap-generated.yaml
```

Expected output:
```
🔗 ADS: Connection established, starting stream
🔄 ADS: Received request for type: type.googleapis.com/envoy.config.cluster.v3.Cluster
📤 ADS: Sending response for type: type.googleapis.com/envoy.config.cluster.v3.Cluster
✅ ADS: Response sent successfully
```

## ⚙️ Configuration

All configuration is centralized in `backend/config.yaml`. See [CONFIGURATION.md](CONFIGURATION.md) for complete details.

### Basic Configuration

```yaml
# Control plane settings
control_plane:
  server:
    rest_port: 8080        # REST API port
    xds_port: 18000        # xDS server port  
    host: "0.0.0.0"        # Binding address
  logging:
    level: "info"          # Log level
  load_balancing:
    default_policy: "ROUND_ROBIN"

# Envoy generation settings
envoy_generation:
  admin:
    host: "127.0.0.1"      # Admin interface
    port: 9901
  listener:
    binding_address: "0.0.0.0"
    default_port: 10000
  cluster:
    connect_timeout_seconds: 5
    discovery_type: "STRICT_DNS"
```

### Configuration Validation

The system performs comprehensive validation on startup:

```bash
# Invalid port (will fail)
control_plane:
  server:
    rest_port: 0  # Error: Port 0 is invalid: rest_port cannot be 0 (reserved)

# Port conflict (will fail)  
control_plane:
  server:
    rest_port: 8080
    xds_port: 8080  # Error: Port conflict: rest_port and xds_port both use port 8080

# Invalid timeout (will fail)
envoy_generation:
  cluster:
    connect_timeout_seconds: 0  # Error: Invalid timeout value 0: cannot be 0

# Short timeout (warning only)
envoy_generation:
  cluster:
    connect_timeout_seconds: 2  # ⚠️ Warning: quite short and may cause connection failures
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

#### Configure Load Balancing
```bash
curl -X POST http://localhost:8080/clusters \
  -H "Content-Type: application/json" \
  -d '{
    "name": "backend-service",
    "endpoints": [{"host": "api.example.com", "port": 443}],
    "load_balancing_policy": "LEAST_REQUEST"
  }'
```

**Supported Policies:** `ROUND_ROBIN`, `LEAST_REQUEST`, `RANDOM`, `RING_HASH`

#### List/Get/Delete Clusters
```bash
# List all clusters
curl http://localhost:8080/clusters

# Get specific cluster
curl http://localhost:8080/clusters/backend-service

# Delete cluster
curl -X DELETE http://localhost:8080/clusters/backend-service
```

### Routes

#### Create Route with URL Rewriting
```bash
curl -X POST http://localhost:8080/routes \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/api/v1/users",
    "cluster_name": "backend-service",
    "prefix_rewrite": "/v2/users"
  }'
```

**Result:** `GET /api/v1/users` → `GET /v2/users` (forwarded to backend-service)

#### List/Get/Delete Routes
```bash
# List all routes
curl http://localhost:8080/routes

# Get specific route  
curl http://localhost:8080/routes/{route-id}

# Delete route
curl -X DELETE http://localhost:8080/routes/{route-id}
```

### Bootstrap Generation

#### Generate Envoy Bootstrap
```bash
curl http://localhost:8080/generate-bootstrap
```

Returns a complete Envoy bootstrap configuration generated from your `config.yaml` settings.

### Health Check
```bash
curl http://localhost:8080/health
# Returns: {"status": "ok"}
```

## 🧪 Testing

### Run All Tests
```bash
# Run unit and integration tests
make backend-test

# Full test suite (includes E2E)
make test-all

# Just E2E tests
make e2e-full
```

### Test Categories

#### Unit Tests
```bash
make backend-test-unit
```

#### Integration Tests  
```bash
make backend-test-integration
```

#### End-to-End Tests
```bash
make e2e-full
```

**E2E Test Flow:**
1. Starts control plane and test backend
2. Generates Envoy bootstrap from config
3. Starts Envoy with generated bootstrap
4. Creates cluster and route via REST API
5. Tests actual HTTP traffic through Envoy
6. Cleans up all resources

### Configuration Validation Tests

```bash
# Test validation with various invalid configs
make backend-test

# Test real validation scenarios
echo 'control_plane: { server: { rest_port: 0 } }' > backend/bad-config.yaml
make backend-dev  # Will show: Error: Port 0 is invalid: rest_port cannot be 0 (reserved)
```

## 🔄 Development Workflow

### Makefile Commands

```bash
# Full Stack Development
make build             # Build both backend and frontend
make dev               # Instructions for running both in development
make test              # Run all tests (backend + frontend)
make clean             # Clean all build artifacts
make format            # Format both backend and frontend code
make lint              # Run linters for both backend and frontend

# Backend Only
make backend-build     # Build the backend application
make backend-dev       # Run backend with debug logging
make backend-test      # Run backend unit and integration tests
make backend-format    # Format backend code
make backend-lint      # Run clippy on backend

# Frontend Only
make frontend-build    # Build the frontend application
make frontend-dev      # Start frontend development server
make frontend-test     # Run frontend tests
make frontend-format   # Format frontend code
make frontend-lint     # Run frontend linter

# Testing
make test-all          # All tests including E2E
make e2e-full          # Complete E2E test suite
make e2e-up            # Start E2E environment
make e2e-down          # Stop and cleanup E2E environment

# Quality
make check             # Format + lint + test (both backend and frontend)
make audit             # Security audit
```

### Docker Support

```bash
# Build and run with Docker
make docker-build
make docker-run

# Or use Docker Compose
docker-compose up
```

## 🏛️ Project Structure

```
backend/                 # Rust control plane
├── src/
│   ├── main.rs              # Application bootstrap
│   ├── config/              # Configuration management
│   │   ├── mod.rs           # YAML config loading
│   │   └── validation.rs    # Comprehensive validation
│   ├── storage/             # Thread-safe data storage
│   │   ├── mod.rs           # Public API
│   │   ├── models.rs        # Data structures
│   │   └── store.rs         # Concurrent storage
│   ├── api/                 # REST API layer  
│   │   ├── mod.rs           # Router and state
│   │   ├── handlers.rs      # HTTP handlers
│   │   └── routes.rs        # Route definitions
│   ├── xds/                 # xDS protocol implementation
│   │   ├── mod.rs           # xDS exports
│   │   ├── simple_server.rs # gRPC server
│   │   └── conversion.rs    # Protobuf conversion
│   └── envoy/              # Envoy config generation
│       └── config_generator.rs # Bootstrap generation
├── tests/                   # Test suite
│   ├── protobuf_conversion_tests.rs
│   ├── rest_api_tests.rs
│   ├── versioning_tests.rs
│   ├── xds_integration_tests.rs
│   └── e2e_integration_tests.rs
├── config.yaml             # Main configuration
├── Cargo.toml              # Rust dependencies
└── Dockerfile              # Backend container

frontend/                # React web interface
├── src/
│   ├── App.tsx              # Main application component
│   ├── pages/               # Page components
│   │   ├── Dashboard.tsx    # Control plane dashboard
│   │   ├── Clusters.tsx     # Cluster management
│   │   ├── Routes.tsx       # Route management
│   │   └── Config.tsx       # Configuration tools
│   └── lib/
│       └── api-client.ts    # Backend API client
├── package.json            # Node.js dependencies
└── vite.config.ts          # Build configuration

docker/                  # Docker compose files
├── docker-compose.test.tls.yml    # E2E testing (TLS)
└── docker-compose.test.plain.yml  # E2E testing (plain)

Makefile                 # Development orchestration
CONFIGURATION.md         # Complete config guide
README.md               # This file
```

## 🔍 Monitoring and Debugging

### Control Plane Logs

```
📈 Version incremented to: 3
📢 Broadcast update notification sent to all connected Envoy instances
🔄 ADS: Pushing resource updates for version: 3
✅ Clusters conversion: Creating 1 clusters
  - Cluster: backend-service (2 endpoints)
📤 ADS: Pushing update for type: type.googleapis.com/envoy.config.cluster.v3.Cluster
✅ ADS: All push updates sent successfully
```

### Envoy Admin Interface

Access at `http://localhost:9901`:
- **`/config_dump`** - Current configuration
- **`/clusters`** - Cluster status and health  
- **`/stats`** - Detailed metrics
- **`/ready`** - Health check endpoint

### Debug Mode

Enable verbose logging:

```yaml
# config.yaml
control_plane:
  logging:
    level: "debug"
```

Or via environment:
```bash
RUST_LOG=debug make backend-dev
```

## 🚨 Troubleshooting

### Configuration Errors

The system provides clear error messages for common issues:

**Port Conflicts:**
```
Error: Port conflict: rest_port and xds_port both use port 8080
Solution: Use different ports for each service
```

**Invalid IP Addresses:**
```
Error: Invalid host address '192.168.1.300': IP address octet 4 (300) must be 0-255
Solution: Use valid IP ranges (0-255 for each octet)
```

**Invalid Timeouts:**
```
Error: Invalid timeout value 0: cluster.connect_timeout_seconds cannot be 0
Solution: Use positive timeout values (1-300 seconds recommended)
```

### Runtime Issues

**Envoy Connection Issues:**
1. Check control plane health: `curl http://localhost:8080/health`
2. Verify Envoy bootstrap has correct control plane address
3. Check for firewall blocking ports 8080/18000

**Route Not Working:**
1. Ensure cluster exists before creating routes
2. Check Envoy admin interface: `http://localhost:9901/config_dump`
3. Verify route path matches your request URL

## 📚 Documentation

- **[CONFIGURATION.md](CONFIGURATION.md)** - Complete configuration reference
- **[API Documentation](#-api-documentation)** - REST API usage examples
- **[Testing Guide](#-testing)** - How to run and write tests

## 🎓 Learning Resources

- [Envoy xDS Protocol](https://www.envoyproxy.io/docs/envoy/latest/api-docs/xds_protocol)
- [Envoy Configuration Reference](https://www.envoyproxy.io/docs/envoy/latest/configuration/configuration)
- [Rust Async Programming](https://rust-lang.github.io/async-book/)
- [gRPC and Tonic Guide](https://github.com/hyperium/tonic)

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Add tests for new functionality
4. Ensure all tests pass: `make test-all`
5. Ensure code quality: `make check`
6. Submit a pull request

### Development Guidelines

- **Configuration First**: All new features should be configurable
- **Validation Required**: Add validation for all new config options
- **Test Coverage**: Include unit, integration, and E2E tests
- **Documentation**: Update both README and CONFIGURATION.md
- **Error Messages**: Provide clear, actionable error messages

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

---

## 🎯 What Makes This Special

This isn't just another control plane implementation. It's designed as a **learning platform** that demonstrates:

- **Production-Quality Validation** - Real-world configuration validation patterns
- **Type-Safe Rust** - Leveraging Rust's type system for reliability  
- **Modern Async Architecture** - Tokio-based concurrent design
- **Comprehensive Testing** - Unit, integration, and full E2E test coverage
- **Zero Hardcoded Values** - Everything configurable and validated
- **Clear Error Messages** - Developer-friendly validation and debugging

Perfect for learning Envoy, Rust async programming, gRPC, and building robust configuration systems!