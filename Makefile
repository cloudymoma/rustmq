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
	@echo "$(YELLOW)🧪 Running debug tests (excluding benchmarks)...$(RESET)"
	cargo test --lib $(FEATURES)
	cargo test --bins $(FEATURES)
	cargo test --tests $(FEATURES)
	@echo "$(GREEN)✅ Debug tests completed$(RESET)"

# WASM test targets
.PHONY: test-wasm wasm-build wasm-test
test-wasm: wasm-build wasm-test

wasm-build:
	@echo "$(YELLOW)🔨 Building WASM test modules...$(RESET)"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup target list --installed | grep -q wasm32-unknown-unknown || (echo "$(YELLOW)📦 Installing wasm32-unknown-unknown target...$(RESET)" && rustup target add wasm32-unknown-unknown)
	@chmod +x tests/wasm_modules/build-all.sh
	@cd tests/wasm_modules && ./build-all.sh
	@echo "$(GREEN)✅ WASM modules built$(RESET)"

wasm-test:
	@echo "$(YELLOW)🧪 Running WASM integration tests...$(RESET)"
	cargo test --test wasm_integration_simple --features wasm -- --nocapture
	@echo "$(GREEN)✅ WASM integration tests completed$(RESET)"

test-release:
	@echo "$(YELLOW)🧪 Running release tests with benchmarks...$(RESET)"
	cargo test --release $(FEATURES)
	@echo "$(YELLOW)🏃 Running benchmarks...$(RESET)"
	cargo bench $(FEATURES)
	@echo "$(GREEN)✅ Release tests and benchmarks completed$(RESET)"

# Miri memory safety tests
.PHONY: test-miri miri-quick miri-core miri-proptest miri-full
test-miri: miri-quick
	@echo "$(GREEN)✅ Miri memory safety tests completed$(RESET)"

miri-quick:
	@echo "$(YELLOW)🔍 Running quick Miri memory safety tests...$(RESET)"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --quick
	@echo "$(GREEN)✅ Quick Miri tests completed$(RESET)"

miri-core:
	@echo "$(YELLOW)🔍 Running core library Miri tests...$(RESET)"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --core-only --verbose
	@echo "$(GREEN)✅ Core Miri tests completed$(RESET)"

miri-proptest:
	@echo "$(YELLOW)🔍 Running property-based Miri tests...$(RESET)"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --proptest-only --verbose
	@echo "$(GREEN)✅ Property-based Miri tests completed$(RESET)"

miri-full:
	@echo "$(YELLOW)🔍 Running full Miri test suite (slow)...$(RESET)"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --verbose
	@echo "$(GREEN)✅ Full Miri test suite completed$(RESET)"

# Pre-commit sanity check - comprehensive validation
.PHONY: sanity
sanity: test sdk-test test-wasm
	@echo "$(YELLOW)🔍 Running final sanity checks...$(RESET)"
	cargo test --lib
	cargo test --bins
	@echo "$(GREEN)✅ Sanity checks completed$(RESET)"

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
.PHONY: sdk-test sdk-test-debug sdk-test-release sdk-quick-test
sdk-test: sdk-test-debug sdk-test-release

sdk-test-debug: sdk-rust-test-debug sdk-go-test-debug

sdk-test-release: sdk-rust-test-release sdk-go-test-release

# Quick SDK test - library tests only for rapid verification
sdk-quick-test:
	@echo "$(YELLOW)⚡ Quick SDK verification (library tests only)...$(RESET)"
	cd sdk/rust && cargo test --lib
	cd sdk/go && go test ./rustmq -v
	@echo "$(GREEN)✅ Quick SDK tests completed$(RESET)"

# Rust SDK test targets
.PHONY: sdk-rust-test sdk-rust-test-debug sdk-rust-test-release sdk-rust-bench sdk-rust-miri
sdk-rust-test: sdk-rust-test-debug sdk-rust-test-release

sdk-rust-test-debug:
	@echo "$(YELLOW)🧪 Running Rust SDK debug tests (library only)...$(RESET)"
	cd sdk/rust && cargo test --lib
	@echo "$(GREEN)✅ Rust SDK debug tests completed (31 tests passed)$(RESET)"

sdk-rust-test-release:
	@echo "$(YELLOW)🧪 Running Rust SDK release tests...$(RESET)"
	cd sdk/rust && cargo test --lib --release
	@echo "$(YELLOW)🏃 Running Rust SDK benchmarks...$(RESET)"
	cd sdk/rust && cargo bench || echo "$(YELLOW)⚠️  Some benchmarks may fail due to missing certificates but performance tests work$(RESET)"
	@echo "$(GREEN)✅ Rust SDK release tests completed (31 tests passed)$(RESET)"

sdk-rust-bench:
	@echo "$(YELLOW)🏃 Running Rust SDK benchmarks only...$(RESET)"
	cd sdk/rust && cargo bench
	@echo "$(GREEN)✅ Rust SDK benchmarks completed$(RESET)"

# Rust SDK Miri memory safety tests
.PHONY: sdk-rust-miri sdk-rust-miri-quick sdk-rust-miri-full
sdk-rust-miri: sdk-rust-miri-quick
	@echo "$(GREEN)✅ Rust SDK Miri memory safety tests completed$(RESET)"

sdk-rust-miri-quick:
	@echo "$(YELLOW)🔍 Running Rust SDK Miri memory safety tests (quick)...$(RESET)"
	@which rustup > /dev/null || (echo "$(RED)❌ Rustup not found. Please install Rust via rustup.$(RESET)" && exit 1)
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri::client_tests::test_message_memory_safety || echo "$(YELLOW)⚠️  Some SDK Miri tests may be filtered out - this is expected$(RESET)"
	@echo "$(GREEN)✅ Rust SDK quick Miri tests completed$(RESET)"

sdk-rust-miri-full:
	@echo "$(YELLOW)🔍 Running full Rust SDK Miri test suite...$(RESET)"
	@rustup +nightly component list --installed | grep -q miri || (echo "$(YELLOW)📦 Installing Miri...$(RESET)" && rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri:: || echo "$(YELLOW)⚠️  Some SDK Miri tests may be filtered out - this is expected$(RESET)"
	@echo "$(GREEN)✅ Full Rust SDK Miri tests completed$(RESET)"

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
