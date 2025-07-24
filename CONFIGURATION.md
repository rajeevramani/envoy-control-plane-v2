# Configuration Guide

This document provides comprehensive information about configuring the Envoy Control Plane.

## Overview

The Envoy Control Plane uses a YAML-based configuration system that eliminates hardcoded values and provides comprehensive validation. All configuration is centralized in `config.yaml`.

## Configuration Structure

The configuration is divided into two main sections:

- **`control_plane`**: Settings for the Rust control plane application
- **`envoy_generation`**: Settings used when generating Envoy configuration files

## Configuration Reference

### Control Plane Configuration

#### Server Settings (`control_plane.server`)

Controls the control plane's network binding and ports.

```yaml
control_plane:
  server:
    rest_port: 8080        # REST API port for cluster/route management
    xds_port: 18000        # xDS server port for Envoy connections  
    host: "0.0.0.0"        # Control plane binding address
```

**Validation Rules:**
- `rest_port` and `xds_port`: Must be 1-65535, cannot be the same
- `host`: Valid IP address or hostname format
- Ports < 1024 will show privilege warnings

#### Logging Settings (`control_plane.logging`)

```yaml
control_plane:
  logging:
    level: "info"          # Control plane log level
```

**Valid Levels:** `error`, `warn`, `info`, `debug`, `trace`

#### Load Balancing Settings (`control_plane.load_balancing`)

```yaml
control_plane:
  load_balancing:
    envoy_version: "1.24"
    available_policies:    # Policies our control plane supports
      - "ROUND_ROBIN"
      - "LEAST_REQUEST" 
      - "RANDOM"
      - "RING_HASH"
    default_policy: "ROUND_ROBIN"  # Default when none specified
```

### Envoy Generation Configuration

These settings control how Envoy configuration files are generated.

#### File Generation (`envoy_generation.config_dir`)

```yaml
envoy_generation:
  config_dir: "./configs"  # Where to write generated Envoy configs
```

#### Admin Interface (`envoy_generation.admin`)

```yaml
envoy_generation:
  admin:
    host: "127.0.0.1"      # Envoy admin interface binding
    port: 9901             # Envoy admin interface port
```

**Validation Rules:**
- `port`: Must be 1-65535
- `host`: Valid IP address or hostname

#### Listener Configuration (`envoy_generation.listener`)

```yaml
envoy_generation:
  listener:
    binding_address: "0.0.0.0"  # Envoy proxy listener binding
    default_port: 10000          # Default Envoy proxy port
```

#### Cluster Configuration (`envoy_generation.cluster`)

```yaml
envoy_generation:
  cluster:
    connect_timeout_seconds: 5   # Cluster connection timeout
    discovery_type: "STRICT_DNS" # Cluster discovery type
    dns_lookup_family: "V4_ONLY" # DNS lookup family  
    default_protocol: "TCP"      # Default endpoint protocol
```

**Validation Rules:**
- `connect_timeout_seconds`: Must be 1-300 seconds (warnings for < 5 seconds)
- `discovery_type`: `STRICT_DNS`, `LOGICAL_DNS`, etc.
- `dns_lookup_family`: `V4_ONLY`, `V6_ONLY`, `AUTO`
- `default_protocol`: `TCP`, `UDP`

#### Naming Configuration (`envoy_generation.naming`)

Controls the names used in generated Envoy configurations.

```yaml
envoy_generation:
  naming:
    listener_name: "listener_0"        # Envoy listener name
    virtual_host_name: "local_service" # Virtual host name
    route_config_name: "local_route"   # Route configuration name
    default_domains: ["*"]             # Default virtual host domains
```

#### Bootstrap Configuration (`envoy_generation.bootstrap`)

Settings for generating Envoy bootstrap files.

```yaml
envoy_generation:
  bootstrap:
    node_id: "envoy-test-node"         # Envoy node identifier
    node_cluster: "envoy-test-cluster" # Envoy node cluster name
    control_plane_host: "control-plane" # Control plane service name
    main_listener_name: "main_listener" # Main proxy listener name
    control_plane_cluster_name: "control_plane_cluster" # Static cluster name
```

#### HTTP Filters Configuration (`envoy_generation.http_filters`)

```yaml
envoy_generation:
  http_filters:
    stat_prefix: "ingress_http"        # HTTP connection manager statistics prefix
    router_filter_name: "envoy.filters.http.router" # HTTP router filter name
    hcm_filter_name: "envoy.filters.network.http_connection_manager" # HCM filter name
```

## Configuration Validation

The system performs comprehensive validation on startup:

### Port Validation
- **Range Check**: All ports must be 1-65535
- **Conflict Detection**: No two services can use the same port
- **Privilege Warning**: Ports < 1024 require root privileges

### Host Validation  
- **IP Address Format**: Valid IPv4 addresses (0.0.0.0 to 255.255.255.255)
- **Hostname Format**: Valid DNS hostname rules
- **Character Validation**: No spaces or invalid characters

### Timeout Validation
- **Range Check**: Timeouts must be 1-300 seconds
- **Zero Detection**: Zero timeouts are invalid
- **Performance Warnings**: Timeouts < 5 seconds may cause issues

### Error Handling
- **Fail Fast**: Invalid configuration stops startup immediately
- **Clear Messages**: All errors explain what's wrong and why
- **Helpful Warnings**: Non-fatal issues show warnings but allow startup

## Example Configurations

### Development Configuration
```yaml
control_plane:
  server:
    rest_port: 8080
    xds_port: 18000
    host: "127.0.0.1"      # Localhost only
  logging:
    level: "debug"         # Verbose logging

envoy_generation:
  cluster:
    connect_timeout_seconds: 10  # Longer timeout for dev
```

### Production Configuration
```yaml
control_plane:
  server:
    rest_port: 8080
    xds_port: 18000  
    host: "0.0.0.0"        # Bind to all interfaces
  logging:
    level: "info"          # Production logging

envoy_generation:
  cluster:
    connect_timeout_seconds: 5   # Optimized timeout
```

### High Security Configuration
```yaml
control_plane:
  server:
    rest_port: 9080        # Non-standard port
    xds_port: 19000        # Non-standard port
    host: "10.0.1.100"     # Specific internal IP
  
envoy_generation:
  admin:
    host: "127.0.0.1"      # Admin only on localhost
    port: 9901
```

## Troubleshooting

### Common Configuration Errors

**Port Conflicts**
```
Error: Port conflict: rest_port and xds_port both use port 8080
```
**Solution:** Use different ports for each service.

**Invalid IP Address**
```
Error: Invalid host address '192.168.1.300': IP address octet 4 (300) must be 0-255
```
**Solution:** Use valid IP address ranges (0-255 for each octet).

**Invalid Timeout**
```
Error: Invalid timeout value 0: cluster.connect_timeout_seconds cannot be 0
```
**Solution:** Use timeout values between 1-300 seconds.

### Validation Warnings

**Privileged Port Warning**
```
⚠️  Warning: rest_port 80 is a privileged port (requires root on Unix systems)
```
**Action:** Either run as root or use ports > 1024.

**Short Timeout Warning**
```
⚠️  Warning: cluster.connect_timeout_seconds 2s is quite short and may cause connection failures
```
**Action:** Consider using timeouts ≥ 5 seconds for reliability.

## Configuration Best Practices

1. **Use Environment-Specific Configs**: Different settings for dev/staging/prod
2. **Document Custom Values**: Comment why you chose specific values
3. **Test Configuration Changes**: Use validation to catch errors early
4. **Monitor Timeout Settings**: Adjust based on network conditions
5. **Secure Binding**: Use specific IPs in production, not 0.0.0.0
6. **Regular Review**: Periodically review and optimize settings

## Dynamic Configuration Updates

The control plane supports dynamic updates for:
- Adding/removing clusters
- Modifying routes
- Updating load balancing policies

Static configuration (ports, timeouts, etc.) requires restart.