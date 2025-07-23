# Build stage
FROM rust:1.82-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifest files first (for better caching)
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src/ src/
COPY proto/ proto/
COPY build.rs ./
COPY config.yaml ./

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false appuser

# Create app directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/envoy-control-plane /app/
COPY --from=builder /app/config.yaml /app/

# Create configs directory
RUN mkdir -p /app/configs && chown -R appuser:appuser /app

# Switch to app user
USER appuser

# Expose ports
EXPOSE 8080 18000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

# Run the application
CMD ["./envoy-control-plane"]