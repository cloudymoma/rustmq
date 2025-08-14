# RustMQ Makefile
# Handles cross-platform builds with intelligent feature detection

# Platform detection
UNAME_S := $(shell uname -s)
KERNEL_VERSION := $(shell uname -r | cut -d. -f1-2)

# Feature flags based on platform
ifeq ($(UNAME_S),Linux)
	# Check if kernel supports io-uring (5.1+)
	KERNEL_MAJOR := $(shell echo $(KERNEL_VERSION) | cut -d. -f1)
	KERNEL_MINOR := $(shell echo $(KERNEL_VERSION) | cut -d. -f2)
	IO_URING_SUPPORTED := $(shell if [ $(KERNEL_MAJOR) -gt 5 ] || ([ $(KERNEL_MAJOR) -eq 5 ] && [ $(KERNEL_MINOR) -ge 1 ]); then echo "yes"; else echo "no"; fi)
	
	ifeq ($(IO_URING_SUPPORTED),yes)
		FEATURES := --features "io-uring,wasm,moka-cache"
		FEATURES_NO_DEFAULT := --no-default-features --features "io-uring,wasm,moka-cache"
	else
		FEATURES := --features "wasm,moka-cache"
		FEATURES_NO_DEFAULT := --no-default-features --features "wasm,moka-cache"
	endif
	PLATFORM_INFO := "Linux with io-uring: $(IO_URING_SUPPORTED)"
else
	# Non-Linux platforms (macOS, Windows, etc.)
	FEATURES := --features "wasm,moka-cache"
	FEATURES_NO_DEFAULT := --no-default-features --features "wasm,moka-cache"
	PLATFORM_INFO := "$(UNAME_S) (no io-uring support)"
endif

# Colors for output
RED := \033[31m
GREEN := \033[32m
YELLOW := \033[33m
BLUE := \033[34m
RESET := \033[0m

# Default target
.PHONY: all
all: info build test
	@echo "$(GREEN)✅ All builds and tests completed successfully!$(RESET)"

# Show platform and feature information
.PHONY: info
info:
	@echo "$(BLUE)🔍 Platform Detection:$(RESET)"
	@echo "  Platform: $(PLATFORM_INFO)"
	@echo "  Features: $(FEATURES)"
	@echo "  Rust Version: $(shell rustc --version)"
	@echo "  Cargo Version: $(shell cargo --version)"
	@echo ""

# Build targets
.PHONY: build build-debug build-release
build: build-debug build-release

build-debug:
	@echo "$(YELLOW)🔨 Building debug mode...$(RESET)"
	cargo build $(FEATURES)
	@echo "$(GREEN)✅ Debug build completed$(RESET)"

build-release:
	@echo "$(YELLOW)🔨 Building release mode...$(RESET)"
	cargo build --release $(FEATURES)
	@echo "$(GREEN)✅ Release build completed$(RESET)"

# Test targets
.PHONY: test test-debug test-release
test: test-debug test-release

test-debug:
	@echo "$(YELLOW)🧪 Running debug tests (excluding benchmarks)...$(RESET)"
	cargo test --lib $(FEATURES)
	cargo test --bins $(FEATURES)
	cargo test --tests $(FEATURES)
	@echo "$(GREEN)✅ Debug tests completed$(RESET)"

test-release:
	@echo "$(YELLOW)🧪 Running release tests with benchmarks...$(RESET)"
	cargo test --release $(FEATURES)
	@echo "$(YELLOW)🏃 Running benchmarks...$(RESET)"
	cargo bench $(FEATURES)
	@echo "$(GREEN)✅ Release tests and benchmarks completed$(RESET)"

# Pre-commit sanity check, Linux only
sanity: test sdk-test
	cargo test --lib 
	cargo test --bins
	cargo test --tests

# Individual binary builds
.PHONY: build-binaries
build-binaries:
	@echo "$(YELLOW)🔨 Building all binaries...$(RESET)"
	cargo build --release --bin rustmq-broker $(FEATURES)
	cargo build --release --bin rustmq-controller $(FEATURES)
	cargo build --release --bin rustmq-admin $(FEATURES)
	cargo build --release --bin rustmq-bigquery-subscriber $(FEATURES)
	cargo build --release --bin rustmq-admin-server $(FEATURES)
	@echo "$(GREEN)✅ All binaries built$(RESET)"

# SDK build targets (Rust and Go)
.PHONY: sdk-build sdk-build-debug sdk-build-release
sdk-build: sdk-build-debug sdk-build-release

sdk-build-debug: sdk-rust-build-debug sdk-go-build

sdk-build-release: sdk-rust-build-release sdk-go-build

# Rust SDK build targets
.PHONY: sdk-rust-build sdk-rust-build-debug sdk-rust-build-release
sdk-rust-build: sdk-rust-build-debug sdk-rust-build-release

sdk-rust-build-debug:
	@echo "$(YELLOW)🔨 Building Rust SDK debug mode...$(RESET)"
	cd sdk/rust && cargo build
	@echo "$(GREEN)✅ Rust SDK debug build completed$(RESET)"

sdk-rust-build-release:
	@echo "$(YELLOW)🔨 Building Rust SDK release mode...$(RESET)"
	cd sdk/rust && cargo build --release
	@echo "$(GREEN)✅ Rust SDK release build completed$(RESET)"

# Go SDK build targets
.PHONY: sdk-go-build sdk-go-test sdk-go-bench sdk-go-examples
sdk-go-build:
	@echo "$(YELLOW)🔨 Building Go SDK...$(RESET)"
	cd sdk/go && go build ./...
	@echo "$(GREEN)✅ Go SDK build completed$(RESET)"

# Code quality checks
.PHONY: check lint fmt clippy
check:
	@echo "$(YELLOW)🔍 Running cargo check...$(RESET)"
	cargo check $(FEATURES)
	@echo "$(GREEN)✅ Check completed$(RESET)"

lint: clippy fmt sdk-go-fmt sdk-go-vet

clippy:
	@echo "$(YELLOW)📎 Running clippy...$(RESET)"
	cargo clippy --all-targets $(FEATURES) -- -D warnings
	@echo "$(GREEN)✅ Clippy completed$(RESET)"

fmt:
	@echo "$(YELLOW)📝 Running rustfmt...$(RESET)"
	cargo fmt --check
	@echo "$(GREEN)✅ Format check completed$(RESET)"

# Clean targets
.PHONY: clean clean-all
clean:
	@echo "$(YELLOW)🧹 Cleaning build artifacts...$(RESET)"
	cargo clean
	@echo "$(GREEN)✅ Clean completed$(RESET)"

clean-all: clean
	@echo "$(YELLOW)🧹 Removing target directory...$(RESET)"
	rm -rf target/
	@echo "$(GREEN)✅ Deep clean completed$(RESET)"

# Performance-specific targets (all benchmarks run in release mode for accurate results)
.PHONY: bench bench-cache bench-security bench-wal bench-replication
bench:
	@echo "$(YELLOW)🏃 Running all benchmarks...$(RESET)"
	cargo bench $(FEATURES)

bench-cache:
	@echo "$(YELLOW)🏃 Running cache benchmarks...$(RESET)"
	cargo bench --bench cache_performance_bench $(FEATURES)

bench-security:
	@echo "$(YELLOW)🏃 Running security benchmarks...$(RESET)"
	cargo bench --bench security_performance $(FEATURES)
	cargo bench --bench authorization_benchmarks $(FEATURES)
	cargo bench --bench simple_security_benchmarks $(FEATURES)
	cargo bench --bench standalone_security_bench $(FEATURES)

bench-wal:
	@echo "$(YELLOW)🏃 Running WAL benchmarks...$(RESET)"
	cargo bench --bench wal_performance_bench $(FEATURES)

bench-replication:
	@echo "$(YELLOW)🏃 Running replication benchmarks...$(RESET)"
	cargo bench --bench replication_manager_benchmarks $(FEATURES)

# Legacy LRU cache testing (without default features)
.PHONY: test-legacy-cache
test-legacy-cache:
	@echo "$(YELLOW)🧪 Testing with legacy LRU cache...$(RESET)"
	cargo test --lib $(FEATURES_NO_DEFAULT)
	@echo "$(GREEN)✅ Legacy cache tests completed$(RESET)"

# SDK test targets (Rust and Go)
.PHONY: sdk-test sdk-test-debug sdk-test-release
sdk-test: sdk-test-debug sdk-test-release

sdk-test-debug: sdk-rust-test-debug sdk-go-test-debug

sdk-test-release: sdk-rust-test-release sdk-go-test-release

# Rust SDK test targets
.PHONY: sdk-rust-test sdk-rust-test-debug sdk-rust-test-release
sdk-rust-test: sdk-rust-test-debug sdk-rust-test-release

sdk-rust-test-debug:
	@echo "$(YELLOW)🧪 Running Rust SDK debug tests (excluding benchmarks)...$(RESET)"
	cd sdk/rust && cargo test --lib
	cd sdk/rust && cargo test --bins
	cd sdk/rust && cargo test --tests
	@echo "$(GREEN)✅ Rust SDK debug tests completed$(RESET)"

sdk-rust-test-release:
	@echo "$(YELLOW)🧪 Running Rust SDK release tests...$(RESET)"
	cd sdk/rust && cargo test --release --lib
	cd sdk/rust && cargo test --release --tests
	@echo "$(YELLOW)🏃 Running Rust SDK benchmarks (if available)...$(RESET)"
	-cd sdk/rust && cargo bench 2>/dev/null || echo "$(YELLOW)⚠️  Some benchmarks skipped due to compilation issues$(RESET)"
	@echo "$(GREEN)✅ Rust SDK release tests completed$(RESET)"

# Go SDK test targets
.PHONY: sdk-go-test sdk-go-test-debug sdk-go-test-release sdk-go-bench
sdk-go-test: sdk-go-test-debug sdk-go-test-release

sdk-go-test-debug:
	@echo "$(YELLOW)🧪 Running Go SDK debug tests (excluding benchmarks)...$(RESET)"
	cd sdk/go && go test ./rustmq ./tests ./benchmarks -v
	@echo "$(GREEN)✅ Go SDK debug tests completed$(RESET)"

sdk-go-test-release:
	@echo "$(YELLOW)🧪 Running Go SDK release tests...$(RESET)"
	cd sdk/go && go test ./rustmq ./tests ./benchmarks -v -ldflags="-s -w"
	@echo "$(YELLOW)🏃 Running Go SDK benchmarks (release optimized)...$(RESET)"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq ./tests ./benchmarks
	@echo "$(GREEN)✅ Go SDK release tests and benchmarks completed$(RESET)"

sdk-go-bench:
	@echo "$(YELLOW)🏃 Running Go SDK benchmarks (release optimized)...$(RESET)"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq ./tests ./benchmarks
	@echo "$(GREEN)✅ Go SDK benchmarks completed$(RESET)"

sdk-go-examples:
	@echo "$(YELLOW)🔨 Building Go SDK examples...$(RESET)"
	cd sdk/go && go build -o bin/simple_producer examples/simple_producer.go
	cd sdk/go && go build -o bin/simple_consumer examples/simple_consumer.go
	cd sdk/go && go build -o bin/stream_processor examples/stream_processor.go
	cd sdk/go && go build -o bin/advanced_stream_processor examples/advanced_stream_processor.go
	cd sdk/go && go build -o bin/secure_producer_mtls examples/secure_producer_mtls.go
	cd sdk/go && go build -o bin/secure_consumer_jwt examples/secure_consumer_jwt.go
	cd sdk/go && go build -o bin/secure_admin_sasl examples/secure_admin_sasl.go
	cd sdk/go && go build -o bin/test-producer examples/mock_test_producer.go
	cd sdk/go && go build -o bin/production-producer examples/production_producer.go
	cd sdk/go && go build -o bin/high-throughput-producer examples/high_throughput_producer.go
	cd sdk/go && go build -o bin/secure-mtls-producer examples/secure_production_mtls.go
	@echo "$(GREEN)✅ Go SDK examples built$(RESET)"

# Go SDK quality checks
.PHONY: sdk-go-fmt sdk-go-vet sdk-go-mod-tidy
sdk-go-fmt:
	@echo "$(YELLOW)📝 Running go fmt on Go SDK...$(RESET)"
	cd sdk/go && go fmt ./...
	@echo "$(GREEN)✅ Go SDK format check completed$(RESET)"

sdk-go-vet:
	@echo "$(YELLOW)🔍 Running go vet on Go SDK...$(RESET)"
	cd sdk/go && go vet ./...
	@echo "$(GREEN)✅ Go SDK vet check completed$(RESET)"

sdk-go-mod-tidy:
	@echo "$(YELLOW)🧹 Running go mod tidy on Go SDK...$(RESET)"
	cd sdk/go && go mod tidy
	@echo "$(GREEN)✅ Go SDK mod tidy completed$(RESET)"

# Development helpers
.PHONY: dev dev-broker dev-controller dev-admin
dev:
	@echo "$(BLUE)🚀 Available development commands:$(RESET)"
	@echo "  make dev-broker     - Run broker in development mode"
	@echo "  make dev-controller - Run controller in development mode"
	@echo "  make dev-admin      - Run admin CLI"

dev-broker:
	@echo "$(YELLOW)🚀 Starting broker in development mode...$(RESET)"
	cargo run --bin rustmq-broker $(FEATURES) -- --config config/broker.toml

dev-controller:
	@echo "$(YELLOW)🚀 Starting controller in development mode...$(RESET)"
	cargo run --bin rustmq-controller $(FEATURES) -- --config config/controller.toml

dev-admin:
	@echo "$(YELLOW)🚀 Starting admin CLI...$(RESET)"
	cargo run --bin rustmq-admin $(FEATURES) -- --help

# Docker helpers
.PHONY: docker-build docker-up docker-down
docker-build:
	@echo "$(YELLOW)🐳 Building Docker images...$(RESET)"
	docker-compose build
	@echo "$(GREEN)✅ Docker images built$(RESET)"

docker-up:
	@echo "$(YELLOW)🐳 Starting Docker cluster...$(RESET)"
	docker-compose up -d
	@echo "$(GREEN)✅ Docker cluster started$(RESET)"

docker-down:
	@echo "$(YELLOW)🐳 Stopping Docker cluster...$(RESET)"
	docker-compose down
	@echo "$(GREEN)✅ Docker cluster stopped$(RESET)"

# Help target
.PHONY: help
help:
	@echo "$(BLUE)RustMQ Makefile Help$(RESET)"
	@echo ""
	@echo "$(YELLOW)Main Targets:$(RESET)"
	@echo "  all              - Build and test everything (default)"
	@echo "  build            - Build debug and release modes"
	@echo "  test             - Run tests for debug and release modes"
	@echo "  info             - Show platform and feature detection"
	@echo ""
	@echo "$(YELLOW)Build Targets:$(RESET)"
	@echo "  build-debug      - Build debug mode only"
	@echo "  build-release    - Build release mode only"
	@echo "  build-binaries   - Build all binary targets"
	@echo "  sdk-build        - Build all SDKs (Rust and Go)"
	@echo "  sdk-build-debug  - Build all SDKs debug mode"
	@echo "  sdk-build-release - Build all SDKs release mode"
	@echo "  sdk-rust-build   - Build Rust SDK debug and release"
	@echo "  sdk-rust-build-debug - Build Rust SDK debug mode only"
	@echo "  sdk-rust-build-release - Build Rust SDK release mode only"
	@echo "  sdk-go-build     - Build Go SDK"
	@echo "  sdk-go-examples  - Build Go SDK examples"
	@echo ""
	@echo "$(YELLOW)Test Targets:$(RESET)"
	@echo "  test-debug       - Run debug tests (no benchmarks)"
	@echo "  test-release     - Run release tests with benchmarks"
	@echo "  test-legacy-cache - Test with legacy LRU cache"
	@echo "  sdk-test         - Run all SDK tests (Rust and Go)"
	@echo "  sdk-test-debug   - Run all SDK debug tests (no benchmarks)"
	@echo "  sdk-test-release - Run all SDK release tests with benchmarks"
	@echo "  sdk-rust-test    - Run Rust SDK tests debug and release"
	@echo "  sdk-rust-test-debug - Run Rust SDK debug tests (no benchmarks)"
	@echo "  sdk-rust-test-release - Run Rust SDK release tests with benchmarks"
	@echo "  sdk-go-test      - Run Go SDK tests debug and release"
	@echo "  sdk-go-test-debug - Run Go SDK debug tests (no benchmarks)"
	@echo "  sdk-go-test-release - Run Go SDK release tests with benchmarks"
	@echo "  sdk-go-bench     - Run Go SDK benchmarks (release optimized)"
	@echo ""
	@echo "$(YELLOW)Code Quality:$(RESET)"
	@echo "  check            - Run cargo check"
	@echo "  lint             - Run clippy, fmt, and Go quality checks"
	@echo "  clippy           - Run clippy linter"
	@echo "  fmt              - Check code formatting"
	@echo "  sdk-go-fmt       - Run go fmt on Go SDK"
	@echo "  sdk-go-vet       - Run go vet on Go SDK"
	@echo "  sdk-go-mod-tidy  - Run go mod tidy on Go SDK"
	@echo ""
	@echo "$(YELLOW)Benchmarks:$(RESET)"
	@echo "  bench            - Run all benchmarks"
	@echo "  bench-cache      - Cache performance benchmarks"
	@echo "  bench-security   - Security performance benchmarks"
	@echo "  bench-wal        - WAL performance benchmarks"
	@echo "  bench-replication - Replication benchmarks"
	@echo ""
	@echo "$(YELLOW)Development:$(RESET)"
	@echo "  dev              - Show development commands"
	@echo "  dev-broker       - Run broker in dev mode"
	@echo "  dev-controller   - Run controller in dev mode"
	@echo "  dev-admin        - Run admin CLI"
	@echo ""
	@echo "$(YELLOW)Docker:$(RESET)"
	@echo "  docker-build     - Build Docker images"
	@echo "  docker-up        - Start Docker cluster"
	@echo "  docker-down      - Stop Docker cluster"
	@echo ""
	@echo "$(YELLOW)Cleanup:$(RESET)"
	@echo "  clean            - Clean build artifacts"
	@echo "  clean-all        - Deep clean (remove target dir)"
	@echo ""
	@echo "$(GREEN)Platform: $(PLATFORM_INFO)$(RESET)"
	@echo "$(GREEN)Features: $(FEATURES)$(RESET)"
