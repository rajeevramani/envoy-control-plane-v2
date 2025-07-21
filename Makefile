# Envoy Control Plane - Development Makefile

.PHONY: help build test clean lint format check docker run-dev run-envoy

# Default target
help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

# Development
build: ## Build the application
	cargo build

build-release: ## Build the application in release mode
	cargo build --release

test: ## Run all tests
	cargo test

test-verbose: ## Run tests with verbose output
	cargo test -- --nocapture

lint: ## Run clippy linter
	cargo clippy --all-targets --all-features -- -D warnings

format: ## Format code with rustfmt
	cargo fmt

format-check: ## Check if code is formatted
	cargo fmt --all -- --check

check: format-check lint test ## Run all checks (format, lint, test)

clean: ## Clean build artifacts
	cargo clean

# Security
audit: ## Run security audit
	cargo audit

# Docker
docker-build: ## Build Docker image
	docker build -t envoy-control-plane .

docker-run: ## Run Docker container
	docker run -p 8080:8080 -p 18000:18000 envoy-control-plane

# Development servers
run-dev: ## Run control plane in development mode
	RUST_LOG=debug cargo run

run-envoy: ## Run Envoy with bootstrap config
	envoy -c envoy-bootstrap.yaml

# Testing helpers
test-cluster: ## Create a test cluster
	curl -X POST http://localhost:8080/clusters \
		-H "Content-Type: application/json" \
		-d '{"name": "test-service", "endpoints": [{"host": "httpbin.org", "port": 80}]}'

test-route: ## Create a test route
	curl -X POST http://localhost:8080/routes \
		-H "Content-Type: application/json" \
		-d '{"path": "/test", "cluster_name": "test-service", "prefix_rewrite": "/get"}'

test-request: ## Test the route through Envoy
	curl http://localhost:10000/test

health-check: ## Check control plane health
	curl http://localhost:8080/health

# Cleanup
clean-all: clean ## Clean everything including Docker images
	docker rmi envoy-control-plane 2>/dev/null || true

# CI/CD simulation
ci-check: format-check lint test audit ## Run all CI checks locally