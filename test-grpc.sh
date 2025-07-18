#!/bin/bash

echo "Testing gRPC server connectivity..."

# Test if the gRPC server is listening
if command -v grpcurl &> /dev/null; then
    echo "📋 Listing available gRPC services:"
    grpcurl -plaintext 127.0.0.1:18000 list
    echo ""
    echo "📋 Listing methods for discovery service:"
    grpcurl -plaintext 127.0.0.1:18000 list envoy.service.discovery.v3.RouteDiscoveryService
else
    echo "❌ grpcurl not found. Install with: brew install grpcurl"
    echo "💡 Alternative: Use netstat to check if port 18000 is listening"
    netstat -an | grep 18000
fi