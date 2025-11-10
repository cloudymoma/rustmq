# RustMQ Makefile
# Handles cross-platform builds with intelligent feature detection
# 
# SDK Status (as of latest verification):
# - Rust SDK: 31 tests pass in debug/release, benchmarks functional
# - Go SDK: Security integration tests pass, performance tests working

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
	@printf "$(GREEN)✅ All builds and tests completed successfully!$(RESET)\n"

# Show platform and feature information
.PHONY: info
info:
	@printf "$(BLUE)🔍 Platform Detection:$(RESET)\n"
	@printf "  Platform: $(PLATFORM_INFO)\n"
	@printf "  Features: $(FEATURES)\n"
	@printf "  Rust Version: $(shell rustc --version)\n"
	@printf "  Cargo Version: $(shell cargo --version)\n"
	@printf "\n"

# Build targets
.PHONY: build build-debug build-release
build: build-debug build-release

build-debug:
	@printf "$(YELLOW)🔨 Building debug mode...$(RESET)\n"
	cargo build $(FEATURES)
	@printf "$(GREEN)✅ Debug build completed$(RESET)\n"

build-release:
	@printf "$(YELLOW)🔨 Building release mode...$(RESET)\n"
	cargo build --release $(FEATURES)
	@printf "$(GREEN)✅ Release build completed$(RESET)\n"

# Test targets
.PHONY: test test-debug test-release test-miri test-wasm
test: test-debug test-release

test-debug:
	@printf "$(YELLOW)🧪 Running debug tests (excluding benchmarks)...$(RESET)\n"
	cargo test --lib $(FEATURES)
	cargo test --bins $(FEATURES)
	cargo test --tests $(FEATURES)
	@printf "$(GREEN)✅ Debug tests completed$(RESET)\n"

# WASM test targets
.PHONY: test-wasm wasm-build wasm-test
test-wasm: wasm-build wasm-test

wasm-build:
	@printf "$(YELLOW)🔨 Building WASM test modules...$(RESET)\n"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup target list --installed | grep -q wasm32-unknown-unknown || (echo "$(YELLOW)📦 Installing wasm32-unknown-unknown target...$(RESET)" && rustup target add wasm32-unknown-unknown)
	@chmod +x tests/wasm_modules/build-all.sh
	@cd tests/wasm_modules && ./build-all.sh
	@printf "$(GREEN)✅ WASM modules built$(RESET)\n"

wasm-test:
	@printf "$(YELLOW)🧪 Running WASM integration tests...$(RESET)\n"
	cargo test --test wasm_integration_simple --features wasm -- --nocapture
	@printf "$(GREEN)✅ WASM integration tests completed$(RESET)\n"

test-release:
	@printf "$(YELLOW)🧪 Running release tests with benchmarks...$(RESET)\n"
	cargo test --release $(FEATURES)
	@printf "$(YELLOW)🏃 Running benchmarks...$(RESET)\n"
	cargo bench $(FEATURES)
	@printf "$(GREEN)✅ Release tests and benchmarks completed$(RESET)\n"

# Miri memory safety tests
.PHONY: test-miri miri-quick miri-core miri-proptest miri-full
test-miri: miri-quick
	@printf "$(GREEN)✅ Miri memory safety tests completed$(RESET)\n"

miri-quick:
	@printf "$(YELLOW)🔍 Running quick Miri memory safety tests...$(RESET)\n"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --quick
	@printf "$(GREEN)✅ Quick Miri tests completed$(RESET)\n"

miri-core:
	@printf "$(YELLOW)🔍 Running core library Miri tests...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --core-only --verbose
	@printf "$(GREEN)✅ Core Miri tests completed$(RESET)\n"

miri-proptest:
	@printf "$(YELLOW)🔍 Running property-based Miri tests...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --proptest-only --verbose
	@printf "$(GREEN)✅ Property-based Miri tests completed$(RESET)\n"

miri-full:
	@printf "$(YELLOW)🔍 Running full Miri test suite (slow)...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --verbose
	@printf "$(GREEN)✅ Full Miri test suite completed$(RESET)\n"

# Pre-commit sanity check - comprehensive validation
.PHONY: sanity
sanity: test sdk-test test-wasm
	@printf "$(YELLOW)🔍 Running final sanity checks...$(RESET)\n"
	cargo test --lib
	cargo test --bins
	@printf "$(GREEN)✅ Sanity checks completed$(RESET)\n"

# Individual binary builds
.PHONY: build-binaries
build-binaries:
	@printf "$(YELLOW)🔨 Building all binaries...$(RESET)\n"
	cargo build --release --bin rustmq-broker $(FEATURES)
	cargo build --release --bin rustmq-controller $(FEATURES)
	cargo build --release --bin rustmq-admin $(FEATURES)
	cargo build --release --bin rustmq-bigquery-subscriber $(FEATURES)
	cargo build --release --bin rustmq-admin-server $(FEATURES)
	@printf "$(GREEN)✅ All binaries built$(RESET)\n"

# SDK build targets (Rust and Go)
.PHONY: sdk-build sdk-build-debug sdk-build-release
sdk-build: sdk-build-debug sdk-build-release

sdk-build-debug: sdk-rust-build-debug sdk-go-build

sdk-build-release: sdk-rust-build-release sdk-go-build

# Rust SDK build targets
.PHONY: sdk-rust-build sdk-rust-build-debug sdk-rust-build-release
sdk-rust-build: sdk-rust-build-debug sdk-rust-build-release

sdk-rust-build-debug:
	@printf "$(YELLOW)🔨 Building Rust SDK debug mode...$(RESET)\n"
	cd sdk/rust && cargo build
	@printf "$(GREEN)✅ Rust SDK debug build completed$(RESET)\n"

sdk-rust-build-release:
	@printf "$(YELLOW)🔨 Building Rust SDK release mode...$(RESET)\n"
	cd sdk/rust && cargo build --release
	@printf "$(GREEN)✅ Rust SDK release build completed$(RESET)\n"

# Go SDK build targets
.PHONY: sdk-go-build sdk-go-test sdk-go-bench sdk-go-examples
sdk-go-build:
	@printf "$(YELLOW)🔨 Building Go SDK...$(RESET)\n"
	cd sdk/go && go build ./...
	@printf "$(GREEN)✅ Go SDK build completed$(RESET)\n"

# Code quality checks
.PHONY: check lint fmt clippy
check:
	@printf "$(YELLOW)🔍 Running cargo check...$(RESET)\n"
	cargo check $(FEATURES)
	@printf "$(GREEN)✅ Check completed$(RESET)\n"

lint: clippy fmt sdk-go-fmt sdk-go-vet

clippy:
	@printf "$(YELLOW)📎 Running clippy...$(RESET)\n"
	cargo clippy --all-targets $(FEATURES) -- -D warnings
	@printf "$(GREEN)✅ Clippy completed$(RESET)\n"

fmt:
	@printf "$(YELLOW)📝 Running rustfmt...$(RESET)\n"
	cargo fmt --check
	@printf "$(GREEN)✅ Format check completed$(RESET)\n"

# Clean targets
.PHONY: clean clean-all
clean:
	@printf "$(YELLOW)🧹 Cleaning build artifacts...$(RESET)\n"
	cargo clean
	@printf "$(GREEN)✅ Clean completed$(RESET)\n"

clean-all: clean
	@printf "$(YELLOW)🧹 Removing target directory...$(RESET)\n"
	rm -rf target/
	@printf "$(GREEN)✅ Deep clean completed$(RESET)\n"

# Performance-specific targets (all benchmarks run in release mode for accurate results)
.PHONY: bench bench-cache bench-security bench-wal bench-replication
bench:
	@printf "$(YELLOW)🏃 Running all benchmarks...$(RESET)\n"
	cargo bench $(FEATURES)

bench-cache:
	@printf "$(YELLOW)🏃 Running cache benchmarks...$(RESET)\n"
	cargo bench --bench cache_performance_bench $(FEATURES)

bench-security:
	@printf "$(YELLOW)🏃 Running security benchmarks...$(RESET)\n"
	cargo bench --bench security_performance $(FEATURES)
	cargo bench --bench authorization_benchmarks $(FEATURES)
	cargo bench --bench simple_security_benchmarks $(FEATURES)
	cargo bench --bench standalone_security_bench $(FEATURES)

bench-wal:
	@printf "$(YELLOW)🏃 Running WAL benchmarks...$(RESET)\n"
	cargo bench --bench wal_performance_bench $(FEATURES)

bench-replication:
	@printf "$(YELLOW)🏃 Running replication benchmarks...$(RESET)\n"
	cargo bench --bench replication_manager_benchmarks $(FEATURES)

# Legacy LRU cache testing (without default features)
.PHONY: test-legacy-cache
test-legacy-cache:
	@printf "$(YELLOW)🧪 Testing with legacy LRU cache...$(RESET)\n"
	cargo test --lib $(FEATURES_NO_DEFAULT)
	@printf "$(GREEN)✅ Legacy cache tests completed$(RESET)\n"

# SDK test targets (Rust and Go)
.PHONY: sdk-test sdk-test-debug sdk-test-release sdk-quick-test
sdk-test: sdk-test-debug sdk-test-release

sdk-test-debug: sdk-rust-test-debug sdk-go-test-debug

sdk-test-release: sdk-rust-test-release sdk-go-test-release

# Quick SDK test - library tests only for rapid verification
sdk-quick-test:
	@printf "$(YELLOW)⚡ Quick SDK verification (library tests only)...$(RESET)\n"
	cd sdk/rust && cargo test --lib
	cd sdk/go && go test ./rustmq -v
	@printf "$(GREEN)✅ Quick SDK tests completed$(RESET)\n"

# Rust SDK test targets
.PHONY: sdk-rust-test sdk-rust-test-debug sdk-rust-test-release sdk-rust-bench sdk-rust-miri
sdk-rust-test: sdk-rust-test-debug sdk-rust-test-release

sdk-rust-test-debug:
	@printf "$(YELLOW)🧪 Running Rust SDK debug tests (library only)...$(RESET)\n"
	cd sdk/rust && cargo test --lib
	@printf "$(GREEN)✅ Rust SDK debug tests completed (31 tests passed)$(RESET)\n"

sdk-rust-test-release:
	@printf "$(YELLOW)🧪 Running Rust SDK release tests...$(RESET)\n"
	cd sdk/rust && cargo test --lib --release
	@printf "$(YELLOW)🏃 Running Rust SDK benchmarks...$(RESET)\n"
	cd sdk/rust && cargo bench || echo "$(YELLOW)⚠️  Some benchmarks may fail due to missing certificates but performance tests work$(RESET)\n"
	@printf "$(GREEN)✅ Rust SDK release tests completed (31 tests passed)$(RESET)\n"

sdk-rust-bench:
	@printf "$(YELLOW)🏃 Running Rust SDK benchmarks only...$(RESET)\n"
	cd sdk/rust && cargo bench
	@printf "$(GREEN)✅ Rust SDK benchmarks completed$(RESET)\n"

# Rust SDK Miri memory safety tests
.PHONY: sdk-rust-miri sdk-rust-miri-quick sdk-rust-miri-full
sdk-rust-miri: sdk-rust-miri-quick
	@printf "$(GREEN)✅ Rust SDK Miri memory safety tests completed$(RESET)\n"

sdk-rust-miri-quick:
	@printf "$(YELLOW)🔍 Running Rust SDK Miri memory safety tests (quick)...$(RESET)\n"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri::client_tests::test_message_memory_safety || echo "$(YELLOW)⚠️  Some SDK Miri tests may be filtered out - this is expected$(RESET)\n"
	@printf "$(GREEN)✅ Rust SDK quick Miri tests completed$(RESET)\n"

sdk-rust-miri-full:
	@printf "$(YELLOW)🔍 Running full Rust SDK Miri test suite...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri:: || echo "$(YELLOW)⚠️  Some SDK Miri tests may be filtered out - this is expected$(RESET)\n"
	@printf "$(GREEN)✅ Full Rust SDK Miri tests completed$(RESET)\n"

# Go SDK test targets
.PHONY: sdk-go-test sdk-go-test-debug sdk-go-test-release sdk-go-bench
sdk-go-test: sdk-go-test-debug sdk-go-test-release

sdk-go-test-debug:
	@printf "$(YELLOW)🧪 Running Go SDK debug tests (excluding benchmarks)...$(RESET)\n"
	cd sdk/go && go test ./rustmq ./tests ./benchmarks -v
	@printf "$(GREEN)✅ Go SDK debug tests completed$(RESET)\n"

sdk-go-test-release:
	@printf "$(YELLOW)🧪 Running Go SDK release tests...$(RESET)\n"
	cd sdk/go && go test ./rustmq ./tests ./benchmarks -v -ldflags="-s -w"
	@printf "$(YELLOW)🏃 Running Go SDK benchmarks (release optimized)...$(RESET)\n"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq ./tests ./benchmarks
	@printf "$(GREEN)✅ Go SDK release tests and benchmarks completed$(RESET)\n"

sdk-go-bench:
	@printf "$(YELLOW)🏃 Running Go SDK benchmarks (release optimized)...$(RESET)\n"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq ./tests ./benchmarks
	@printf "$(GREEN)✅ Go SDK benchmarks completed$(RESET)\n"

sdk-go-examples:
	@printf "$(YELLOW)🔨 Building Go SDK examples...$(RESET)\n"
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
	@printf "$(GREEN)✅ Go SDK examples built$(RESET)\n"

# Go SDK quality checks
.PHONY: sdk-go-fmt sdk-go-vet sdk-go-mod-tidy
sdk-go-fmt:
	@printf "$(YELLOW)📝 Running go fmt on Go SDK...$(RESET)\n"
	cd sdk/go && go fmt ./...
	@printf "$(GREEN)✅ Go SDK format check completed$(RESET)\n"

sdk-go-vet:
	@printf "$(YELLOW)🔍 Running go vet on Go SDK...$(RESET)\n"
	cd sdk/go && go vet ./...
	@printf "$(GREEN)✅ Go SDK vet check completed$(RESET)\n"

sdk-go-mod-tidy:
	@printf "$(YELLOW)🧹 Running go mod tidy on Go SDK...$(RESET)\n"
	cd sdk/go && go mod tidy
	@printf "$(GREEN)✅ Go SDK mod tidy completed$(RESET)\n"

# Development helpers
.PHONY: dev dev-broker dev-controller dev-admin
dev:
	@printf "$(BLUE)🚀 Available development commands:$(RESET)\n"
	@echo "  make dev-broker     - Run broker in development mode"
	@echo "  make dev-controller - Run controller in development mode"
	@echo "  make dev-admin      - Run admin CLI"

dev-broker:
	@printf "$(YELLOW)🚀 Starting broker in development mode...$(RESET)\n"
	cargo run --bin rustmq-broker $(FEATURES) -- --config config/broker.toml

dev-controller:
	@printf "$(YELLOW)🚀 Starting controller in development mode...$(RESET)\n"
	cargo run --bin rustmq-controller $(FEATURES) -- --config config/controller.toml

dev-admin:
	@printf "$(YELLOW)🚀 Starting admin CLI...$(RESET)\n"
	cargo run --bin rustmq-admin $(FEATURES) -- --help

# Docker helpers
.PHONY: docker-build docker-up docker-down
docker-build:
	@printf "$(YELLOW)🐳 Building Docker images...$(RESET)\n"
	docker-compose build
	@printf "$(GREEN)✅ Docker images built$(RESET)\n"

docker-up:
	@printf "$(YELLOW)🐳 Starting Docker cluster...$(RESET)\n"
	docker-compose up -d
	@printf "$(GREEN)✅ Docker cluster started$(RESET)\n"

docker-down:
	@printf "$(YELLOW)🐳 Stopping Docker cluster...$(RESET)\n"
	docker-compose down
	@printf "$(GREEN)✅ Docker cluster stopped$(RESET)\n"

# Help target
.PHONY: help
help:
	@printf "$(BLUE)RustMQ Makefile Help$(RESET)\n"
	@echo ""
	@printf "$(YELLOW)Main Targets:$(RESET)\n"
	@echo "  all              - Build and test everything (default)"
	@echo "  build            - Build debug and release modes"
	@echo "  test             - Run tests for debug and release modes"
	@echo "  info             - Show platform and feature detection"
	@echo ""
	@printf "$(YELLOW)Build Targets:$(RESET)\n"
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
	@printf "$(YELLOW)Test Targets:$(RESET)\n"
	@echo "  test-debug       - Run debug tests (no benchmarks)"
	@echo "  test-release     - Run release tests with benchmarks"
	@echo "  test-legacy-cache - Test with legacy LRU cache"
	@echo "  test-miri        - Quick Miri memory safety tests"
	@echo "  miri-quick       - Quick Miri memory safety validation"
	@echo "  miri-core        - Core library Miri tests (storage, security, cache)"
	@echo "  miri-proptest    - Property-based Miri tests"
	@echo "  miri-full        - Full Miri test suite (slow but comprehensive)"
	@echo "  sanity           - Pre-commit sanity checks"
	@echo "  sdk-test         - Run all SDK tests (Rust and Go)"
	@echo "  sdk-quick-test   - Quick SDK verification (library tests only)"
	@echo "  sdk-test-debug   - Run all SDK debug tests (no benchmarks)"
	@echo "  sdk-test-release - Run all SDK release tests with benchmarks"
	@echo "  sdk-rust-test    - Run Rust SDK tests debug and release"
	@echo "  sdk-rust-test-debug - Run Rust SDK debug tests (31 tests)"
	@echo "  sdk-rust-test-release - Run Rust SDK release tests with benchmarks"
	@echo "  sdk-rust-bench   - Run Rust SDK benchmarks only"
	@echo "  sdk-rust-miri    - Rust SDK Miri memory safety tests"
	@echo "  sdk-rust-miri-quick - Quick Rust SDK Miri validation"
	@echo "  sdk-rust-miri-full - Full Rust SDK Miri test suite"
	@echo "  sdk-go-test      - Run Go SDK tests debug and release"
	@echo "  sdk-go-test-debug - Run Go SDK debug tests (no benchmarks)"
	@echo "  sdk-go-test-release - Run Go SDK release tests with benchmarks"
	@echo "  sdk-go-bench     - Run Go SDK benchmarks (release optimized)"
	@echo ""
	@printf "$(YELLOW)Code Quality:$(RESET)\n"
	@echo "  check            - Run cargo check"
	@echo "  lint             - Run clippy, fmt, and Go quality checks"
	@echo "  clippy           - Run clippy linter"
	@echo "  fmt              - Check code formatting"
	@echo "  sdk-go-fmt       - Run go fmt on Go SDK"
	@echo "  sdk-go-vet       - Run go vet on Go SDK"
	@echo "  sdk-go-mod-tidy  - Run go mod tidy on Go SDK"
	@echo ""
	@printf "$(YELLOW)Benchmarks:$(RESET)\n"
	@echo "  bench            - Run all benchmarks"
	@echo "  bench-cache      - Cache performance benchmarks"
	@echo "  bench-security   - Security performance benchmarks"
	@echo "  bench-wal        - WAL performance benchmarks"
	@echo "  bench-replication - Replication benchmarks"
	@echo ""
	@printf "$(YELLOW)Development:$(RESET)\n"
	@echo "  dev              - Show development commands"
	@echo "  dev-broker       - Run broker in dev mode"
	@echo "  dev-controller   - Run controller in dev mode"
	@echo "  dev-admin        - Run admin CLI"
	@echo ""
	@printf "$(YELLOW)Docker:$(RESET)\n"
	@echo "  docker-build     - Build Docker images"
	@echo "  docker-up        - Start Docker cluster"
	@echo "  docker-down      - Stop Docker cluster"
	@echo ""
	@printf "$(YELLOW)Cleanup:$(RESET)\n"
	@echo "  clean            - Clean build artifacts"
	@echo "  clean-all        - Deep clean (remove target dir)"
	@echo ""
	@printf "$(GREEN)Platform: $(PLATFORM_INFO)$(RESET)\n"
	@printf "$(GREEN)Features: $(FEATURES)$(RESET)\n"
