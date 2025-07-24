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

build-release-linux: ## Build the application for Linux (for Docker)
	cargo build --release --target aarch64-unknown-linux-gnu

test: ## Run all unit and integration tests (excludes E2E)
	cargo test

test-verbose: ## Run tests with verbose output
	cargo test -- --nocapture

test-unit: ## Run unit tests only (fast)
	cargo test --lib

test-integration: ## Run integration tests only
	cargo test --test protobuf_conversion_tests
	cargo test --test rest_api_tests
	cargo test --test versioning_tests
	cargo test --test xds_integration_tests

test-all: ## Run all tests including E2E
	@echo "🧪 Running complete test suite..."
	@echo "📋 Step 1: Unit and integration tests..."
	@make test
	@echo "📋 Step 2: E2E tests..."
	@make e2e-full
	@echo "✅ All tests completed!"

lint: ## Run clippy linter
	cargo clippy --all-targets --all-features -- -D warnings

format: ## Format code with rustfmt
	cargo fmt

format-check: ## Check if code is formatted
	cargo fmt --all -- --check

check: format-check lint test ## Run all checks (format, lint, unit+integration tests)

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

# E2E Testing
e2e-generate-bootstrap: ## Generate Envoy bootstrap configuration from our config
	@echo "🔧 Generating Envoy bootstrap configuration..."
	@mkdir -p tests/e2e
	@curl -s http://localhost:8080/generate-bootstrap | jq -r '.data' > tests/e2e/envoy-bootstrap-generated.yaml
	@echo "✅ Bootstrap generated at tests/e2e/envoy-bootstrap-generated.yaml"

e2e-up: ## Start E2E test environment with generated bootstrap
	@echo "🚀 Starting E2E environment with generated bootstrap..."
	@echo "📋 Step 1: Starting control plane and test backend..."
	@docker-compose -f docker-compose.test.yml up --build -d control-plane test-backend
	@echo "⏳ Waiting for control plane to be ready..."
	@sleep 10
	@echo "🔧 Step 2: Generating Envoy bootstrap from control plane config..."
	@make e2e-generate-bootstrap
	@echo "🚀 Step 3: Starting Envoy with generated bootstrap..."
	@docker-compose -f docker-compose.test.yml up -d envoy
	@echo "✅ E2E environment ready!"

e2e-down: ## Stop E2E test environment and clean up generated files
	@echo "🧹 Cleaning up E2E environment..."
	docker-compose -f docker-compose.test.yml down --volumes --remove-orphans
	@echo "🗑️  Removing generated bootstrap file..."
	@rm -f tests/e2e/envoy-bootstrap-generated.yaml
	@echo "✅ E2E environment cleaned up!"

e2e-test: ## Run E2E tests (assumes services are running)
	cargo test --test e2e_integration_tests -- --ignored --nocapture

e2e-full: ## Run complete E2E test suite  
	@echo "🚀 Starting complete E2E test suite..."
	@make e2e-up
	@echo "⏳ Waiting for Envoy to be ready..."
	@sleep 5
	@echo "🧪 Running E2E tests..."
	@make e2e-test || (make e2e-down && exit 1)
	@echo "🧹 Cleaning up E2E environment..."
	@make e2e-down
	@echo "✅ E2E test suite completed!"

e2e-logs: ## Show E2E service logs
	docker-compose -f docker-compose.test.yml logs

# CI/CD simulation
ci-check: format-check lint test audit ## Run all CI checks locally