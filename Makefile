# Envoy Control Plane - Development Makefile
# Orchestrates both backend (Rust) and frontend (React) builds

.PHONY: help build test clean lint format check docker run-dev run-envoy
.PHONY: frontend-build frontend-dev frontend-test frontend-clean frontend-lint backend-build backend-test backend-clean backend-lint

# Default target
help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

# Full stack development
build: backend-build frontend-build ## Build both backend and frontend

test: backend-test frontend-test ## Run all tests for backend and frontend

clean: backend-clean frontend-clean ## Clean build artifacts for both backend and frontend

lint: backend-lint frontend-lint ## Run linters for both backend and frontend

format: backend-format frontend-format ## Format code for both backend and frontend

check: backend-check frontend-check ## Run all checks for both backend and frontend

# Backend commands (Rust)
backend-build: ## Build the backend application
	cd backend && cargo build

backend-build-release: ## Build the backend in release mode
	cd backend && cargo build --release

backend-build-release-linux: ## Build the backend for Linux (for Docker)
	cd backend && cargo build --release --target aarch64-unknown-linux-gnu

backend-test: ## Run all backend unit and integration tests (excludes E2E)
	cd backend && cargo test

backend-test-verbose: ## Run backend tests with verbose output
	cd backend && cargo test -- --nocapture

backend-test-unit: ## Run backend unit tests only (fast)
	cd backend && cargo test --lib

backend-test-integration: ## Run backend integration tests only
	cd backend && cargo test --test protobuf_conversion_tests
	cd backend && cargo test --test rest_api_tests
	cd backend && cargo test --test versioning_tests
	cd backend && cargo test --test xds_integration_tests

backend-lint: ## Run clippy linter on backend
	cd backend && cargo clippy --all-targets --all-features -- -D warnings

backend-format: ## Format backend code with rustfmt
	cd backend && cargo fmt

backend-format-check: ## Check if backend code is formatted
	cd backend && cargo fmt --all -- --check

backend-check: backend-format-check backend-lint backend-test ## Run all backend checks

backend-clean: ## Clean backend build artifacts
	cd backend && cargo clean

# Frontend commands (React/TypeScript)
frontend-install: ## Install frontend dependencies
	cd frontend && npm install

frontend-build: frontend-install ## Build the frontend application
	cd frontend && npm run build

frontend-dev: frontend-install ## Start frontend development server
	cd frontend && npm run dev

frontend-test: frontend-install ## Run frontend tests
	cd frontend && npm run test

frontend-lint: frontend-install ## Run frontend linter
	cd frontend && npm run lint

frontend-format: frontend-install ## Format frontend code
	cd frontend && npm run format

frontend-check: frontend-lint frontend-test ## Run all frontend checks

frontend-clean: ## Clean frontend build artifacts and node_modules
	cd frontend && rm -rf dist node_modules

# Development shortcuts
build-release: backend-build-release frontend-build ## Build both backend and frontend in release mode

test-all: backend-test frontend-test e2e-full ## Run all tests including E2E

dev: ## Start both backend and frontend in development mode (separate terminals needed)
	@echo "🚀 Starting development servers..."
	@echo "📋 Run in separate terminals:"
	@echo "   Terminal 1: make backend-dev"
	@echo "   Terminal 2: make frontend-dev"

backend-dev: ## Run backend in development mode
	cd backend && RUST_LOG=debug cargo run --bin envoy-control-plane

frontend-dev-only: ## Start only frontend dev server (assumes backend is running)
	cd frontend && npm run dev

# Security
audit: ## Run security audit on backend
	cd backend && cargo audit

generate-certs: ## Generate TLS certificates for local development
	@echo "🔐 Generating TLS certificates..."
	@mkdir -p backend/certs
	@cd backend && cargo run --bin cert-generator
	@echo "✅ TLS certificates ready!"

# Docker
docker-build: ## Build Docker image
	docker build -f backend/Dockerfile -t envoy-control-plane .

docker-run: ## Run Docker container
	docker run -p 8080:8080 -p 18000:18000 envoy-control-plane

# Development servers (legacy aliases)
run-dev: ## Run control plane in development mode (alias for backend-dev)
	cd backend && RUST_LOG=debug cargo run --bin envoy-control-plane

run-envoy: ## Run Envoy with bootstrap config
	envoy -c backend/envoy-bootstrap.yaml

run-envoy-tls: ## Run Envoy with TLS-enabled bootstrap config
	envoy -c backend/envoy-bootstrap-tls.yaml

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
clean-all: clean clean-certs ## Clean everything including Docker images and certificates
	docker rmi envoy-control-plane 2>/dev/null || true

# E2E Testing
check-tls-config: ## Check if TLS is enabled in config.yaml (for local development)
	@if grep -A 4 "tls:" backend/config.yaml | grep -q "enabled: true"; then \
		echo "✅ TLS is ENABLED in backend/config.yaml"; \
		echo "TLS_ENABLED=true" > .env.test; \
	else \
		echo "🔓 TLS is DISABLED in backend/config.yaml"; \
		echo "TLS_ENABLED=false" > .env.test; \
	fi

check-e2e-tls-config: ## Check if TLS is enabled in config.e2e.yaml (for e2e testing)
	@if grep -A 4 "tls:" backend/config.e2e.yaml | grep -q "enabled: true"; then \
		echo "✅ TLS is ENABLED in backend/config.e2e.yaml"; \
		echo "TLS_ENABLED=true" > .env.test; \
	else \
		echo "🔓 TLS is DISABLED in backend/config.e2e.yaml"; \
		echo "TLS_ENABLED=false" > .env.test; \
	fi

e2e-enable-tls: ## Enable TLS in config.e2e.yaml for testing
	@echo "🔒 Enabling TLS in backend/config.e2e.yaml..."
	@sed -i '' 's/enabled: false/enabled: true/' backend/config.e2e.yaml
	@sed -i '' 's/enabled:false/enabled: true/' backend/config.e2e.yaml
	@echo "✅ TLS enabled in e2e config"

e2e-disable-tls: ## Disable TLS in config.e2e.yaml for testing
	@echo "🔓 Disabling TLS in backend/config.e2e.yaml..."
	@sed -i '' 's/enabled: true/enabled: false/' backend/config.e2e.yaml
	@sed -i '' 's/enabled:true/enabled: false/' backend/config.e2e.yaml
	@echo "✅ TLS disabled in e2e config"

check-certs: ## Verify TLS certificates exist
	@if [ ! -d "backend/certs" ]; then \
		echo "❌ Certificate directory not found. Run 'make generate-certs' first."; \
		exit 1; \
	fi
	@if [ ! -f "backend/certs/server.crt" ] || [ ! -f "backend/certs/server.key" ]; then \
		echo "❌ Certificate files missing. Run 'make generate-certs' first."; \
		exit 1; \
	fi
	@echo "✅ TLS certificates found and ready!"

e2e-generate-certs: ## Generate TLS certificates for E2E testing (only if needed)
	@echo "🔐 Generating TLS certificates for E2E testing..."
	@mkdir -p backend/certs
	@cd backend && cargo run --bin cert-generator
	@echo "✅ TLS certificates ready for E2E tests"

e2e-generate-bootstrap: ## Generate Envoy bootstrap configuration from our config
	@echo "🔧 Generating Envoy bootstrap configuration..."
	@mkdir -p backend/tests/e2e
	@. .env.test && if [ "$$TLS_ENABLED" = "true" ]; then \
		echo "📋 Generating TLS-enabled bootstrap..."; \
		curl -s http://localhost:8080/generate-bootstrap | jq -r '.data' > backend/tests/e2e/envoy-bootstrap-tls.yaml; \
		echo "✅ TLS bootstrap generated at backend/tests/e2e/envoy-bootstrap-tls.yaml"; \
	else \
		echo "📋 Generating plain HTTP bootstrap..."; \
		curl -s http://localhost:8080/generate-bootstrap | jq -r '.data' > backend/tests/e2e/envoy-bootstrap-plain.yaml; \
		echo "✅ Plain bootstrap generated at backend/tests/e2e/envoy-bootstrap-plain.yaml"; \
	fi

e2e-up: ## Start E2E test environment with generated bootstrap
	@echo "🚀 Starting E2E environment with generated bootstrap..."
	@echo "🔍 Step 1: Checking E2E TLS configuration..."
	@make check-e2e-tls-config
	@. .env.test && if [ "$$TLS_ENABLED" = "true" ]; then \
		echo "🔐 Step 2: Generating TLS certificates..."; \
		make e2e-generate-certs; \
		echo "🔍 Step 2b: Verifying certificates..."; \
		make check-certs; \
		echo "📋 Step 3: Starting TLS-enabled control plane and test backend..."; \
		docker-compose -f docker/docker-compose.test.tls.yml up --build -d control-plane test-backend; \
	else \
		echo "📋 Step 2: Starting plain HTTP control plane and test backend..."; \
		docker-compose -f docker/docker-compose.test.plain.yml up --build -d control-plane test-backend; \
	fi
	@echo "⏳ Waiting for control plane to be ready..."
	@sleep 10
	@echo "🔧 Step 4: Generating Envoy bootstrap from control plane config..."
	@make e2e-generate-bootstrap
	@. .env.test && if [ "$$TLS_ENABLED" = "true" ]; then \
		echo "🚀 Step 5: Starting Envoy with TLS bootstrap..."; \
		docker-compose -f docker/docker-compose.test.tls.yml up -d envoy; \
	else \
		echo "🚀 Step 5: Starting Envoy with plain bootstrap..."; \
		docker-compose -f docker/docker-compose.test.plain.yml up -d envoy; \
	fi
	@echo "✅ E2E environment ready!"

e2e-down: ## Stop E2E test environment and clean up generated files
	@echo "🧹 Cleaning up E2E environment..."
	@echo "🛑 Stopping TLS environment (if running)..."
	@docker-compose -f docker/docker-compose.test.tls.yml down --volumes --remove-orphans 2>/dev/null || true
	@echo "🛑 Stopping plain environment (if running)..."
	@docker-compose -f docker/docker-compose.test.plain.yml down --volumes --remove-orphans 2>/dev/null || true
	@echo "🗑️  Removing generated bootstrap files..."
	@rm -f backend/tests/e2e/envoy-bootstrap-tls.yaml
	@rm -f backend/tests/e2e/envoy-bootstrap-plain.yaml
	@rm -f .env.test
	@echo "✅ E2E environment cleaned up!"

clean-certs: ## Remove generated TLS certificates
	@echo "🗑️  Removing TLS certificates..."
	@rm -rf backend/certs/
	@echo "✅ TLS certificates cleaned up!"

e2e-test: ## Run E2E tests (assumes services are running)
	cd backend && cargo test --test e2e_integration_tests -- --ignored --nocapture

e2e-full: ## Run complete E2E test suite (uses current TLS setting in config.e2e.yaml)
	@echo "🚀 Starting complete E2E test suite..."
	@make e2e-up
	@echo "⏳ Waiting for Envoy to be ready..."
	@sleep 5
	@echo "🧪 Running E2E tests..."
	@make e2e-test || (make e2e-down && exit 1)
	@echo "🧹 Cleaning up E2E environment..."
	@make e2e-down
	@echo "✅ E2E test suite completed!"

e2e-full-tls: e2e-test-tls ## Alias for e2e-test-tls (consistency with e2e-full naming)

e2e-full-plain: e2e-test-plain ## Alias for e2e-test-plain (consistency with e2e-full naming)

e2e-test-tls: ## Run E2E tests with TLS enabled
	@echo "🔒 Testing E2E with TLS enabled..."
	@make e2e-enable-tls
	@make e2e-full
	@echo "✅ TLS E2E test completed!"

e2e-test-plain: ## Run E2E tests with TLS disabled
	@echo "🔓 Testing E2E with TLS disabled..."
	@make e2e-disable-tls
	@make e2e-full
	@echo "✅ Plain HTTP E2E test completed!"

e2e-test-both: ## Run E2E tests for both TLS and plain HTTP scenarios
	@echo "🧪 Running comprehensive E2E tests (both TLS and plain HTTP)..."
	@echo "📋 Test 1: TLS enabled scenario"
	@make e2e-test-tls
	@echo "📋 Test 2: Plain HTTP scenario"
	@make e2e-test-plain
	@echo "🎉 All E2E tests completed successfully!"

e2e-logs: ## Show E2E service logs
	@if [ -f .env.test ]; then \
		. .env.test && if [ "$$TLS_ENABLED" = "true" ]; then \
			echo "📋 Showing TLS environment logs..."; \
			docker-compose -f docker/docker-compose.test.tls.yml logs; \
		else \
			echo "📋 Showing plain environment logs..."; \
			docker-compose -f docker/docker-compose.test.plain.yml logs; \
		fi; \
	else \
		echo "❌ No environment detected. Run 'make check-tls-config' first."; \
	fi

# CI/CD simulation
ci-check: backend-format-check backend-lint backend-test audit ## Run all CI checks locally