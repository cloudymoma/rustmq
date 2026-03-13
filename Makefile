# RustMQ Makefile
# Handles cross-platform builds with intelligent feature detection
# Optimized for parallel execution on multi-core machines

# Platform detection
UNAME_S := $(shell uname -s)
KERNEL_VERSION := $(shell uname -r | cut -d. -f1-2)
NPROC := $(shell nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Feature flags based on platform
ifeq ($(UNAME_S),Linux)
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
	PLATFORM_INFO := Linux, $(NPROC) cores, io-uring: $(IO_URING_SUPPORTED)
else
	FEATURES := --features "wasm,moka-cache"
	FEATURES_NO_DEFAULT := --no-default-features --features "wasm,moka-cache"
	PLATFORM_INFO := $(UNAME_S), $(NPROC) cores, no io-uring
endif

# Parallel test execution for cargo (number of test threads)
TEST_THREADS := $(NPROC)

# Colors for output
RED := \033[31m
GREEN := \033[32m
YELLOW := \033[33m
BLUE := \033[34m
RESET := \033[0m

# ============================================================
# Primary targets
# ============================================================

.PHONY: all
all: info build test
	@printf "$(GREEN)All builds and tests completed successfully!$(RESET)\n"

# Show platform and feature information
.PHONY: info
info:
	@printf "$(BLUE)Platform Detection:$(RESET)\n"
	@printf "  Platform: $(PLATFORM_INFO)\n"
	@printf "  Features: $(FEATURES)\n"
	@printf "  Rust: $(shell rustc --version)\n"
	@printf "  Cargo: $(shell cargo --version)\n"
	@printf "\n"

# ============================================================
# Build targets
# ============================================================

.PHONY: build build-debug build-release
build: build-debug build-release

build-debug:
	@printf "$(YELLOW)Building debug mode ($(NPROC) jobs)...$(RESET)\n"
	cargo build -j $(NPROC) $(FEATURES)
	@printf "$(GREEN)Debug build completed$(RESET)\n"

build-release:
	@printf "$(YELLOW)Building release mode ($(NPROC) jobs)...$(RESET)\n"
	cargo build --release -j $(NPROC) $(FEATURES)
	@printf "$(GREEN)Release build completed$(RESET)\n"

# ============================================================
# Test targets
# ============================================================

.PHONY: test test-debug test-release
test: test-debug test-release

test-debug:
	@printf "$(YELLOW)Running debug tests ($(TEST_THREADS) threads)...$(RESET)\n"
	cargo test -j $(NPROC) $(FEATURES) -- --test-threads=$(TEST_THREADS)
	@printf "$(GREEN)Debug tests completed$(RESET)\n"

test-release:
	@printf "$(YELLOW)Running release tests ($(TEST_THREADS) threads)...$(RESET)\n"
	cargo test --release -j $(NPROC) $(FEATURES) -- --test-threads=$(TEST_THREADS)
	@printf "$(GREEN)Release tests completed$(RESET)\n"

# ============================================================
# Pre-commit sanity check — parallelized
# ============================================================
# Runs independent stages concurrently using make -j:
#   Stage 1 (parallel): debug build + test, Go SDK test, WASM test
#   Stage 2 (sequential): release build + test + benchmarks (needs CPU isolation)
#
# Usage: make sanity          (parallel stages)
#        make sanity -j4      (explicit parallelism)

.PHONY: sanity sanity-parallel sanity-sequential
sanity:
	@printf "$(BLUE)=== RustMQ Pre-commit Sanity Check ===$(RESET)\n"
	@printf "$(BLUE)Stage 1: Parallel validation (debug + SDK + WASM)...$(RESET)\n"
	$(MAKE) sanity-parallel -j3
	@printf "$(BLUE)Stage 2: Release build, tests & benchmarks (sequential for accuracy)...$(RESET)\n"
	$(MAKE) sanity-sequential
	@printf "$(GREEN)All sanity checks passed!$(RESET)\n"

# Stage 1: These three targets are independent and run in parallel
sanity-parallel: _sanity-debug _sanity-sdk _sanity-wasm

_sanity-debug:
	@printf "$(YELLOW)[parallel] Debug build + tests...$(RESET)\n"
	cargo build -j $(NPROC) $(FEATURES)
	cargo test -j $(NPROC) --lib $(FEATURES) -- --test-threads=$(TEST_THREADS)
	cargo test -j $(NPROC) --bins $(FEATURES) -- --test-threads=$(TEST_THREADS)
	cargo test -j $(NPROC) --tests $(FEATURES) -- --test-threads=$(TEST_THREADS)
	@printf "$(GREEN)[parallel] Debug tests passed$(RESET)\n"

_sanity-sdk:
	@printf "$(YELLOW)[parallel] SDK tests...$(RESET)\n"
	cd sdk/rust && cargo test -j $(NPROC) --lib
	cd sdk/go && go test -parallel $(TEST_THREADS) ./rustmq/... ./tests/...
	@printf "$(GREEN)[parallel] SDK tests passed$(RESET)\n"

_sanity-wasm:
	@printf "$(YELLOW)[parallel] WASM tests...$(RESET)\n"
	@rustup target list --installed | grep -q wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
	@if [ -f tests/wasm_modules/build-all.sh ]; then chmod +x tests/wasm_modules/build-all.sh && cd tests/wasm_modules && ./build-all.sh; fi
	cargo test -j $(NPROC) --test wasm_integration_simple --features wasm -- --nocapture
	@printf "$(GREEN)[parallel] WASM tests passed$(RESET)\n"

# Stage 2: Release tests and benchmarks run sequentially for accurate results
sanity-sequential:
	@printf "$(YELLOW)Release build + tests...$(RESET)\n"
	cargo build --release -j $(NPROC) $(FEATURES)
	cargo test --release -j $(NPROC) $(FEATURES) -- --test-threads=$(TEST_THREADS)
	@printf "$(YELLOW)Running benchmarks (sequential, CPU-isolated)...$(RESET)\n"
	cargo bench -j $(NPROC) $(FEATURES)
	@printf "$(GREEN)Release tests and benchmarks completed$(RESET)\n"

# ============================================================
# WASM test targets
# ============================================================

.PHONY: test-wasm wasm-build wasm-test
test-wasm: wasm-build wasm-test

wasm-build:
	@printf "$(YELLOW)Building WASM test modules...$(RESET)\n"
	@rustup target list --installed | grep -q wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
	@if [ -f tests/wasm_modules/build-all.sh ]; then chmod +x tests/wasm_modules/build-all.sh && cd tests/wasm_modules && ./build-all.sh; fi
	@printf "$(GREEN)WASM modules built$(RESET)\n"

wasm-test:
	@printf "$(YELLOW)Running WASM integration tests...$(RESET)\n"
	cargo test -j $(NPROC) --test wasm_integration_simple --features wasm -- --nocapture
	@printf "$(GREEN)WASM integration tests completed$(RESET)\n"

# ============================================================
# Miri memory safety tests
# ============================================================

.PHONY: test-miri miri-quick miri-core miri-proptest miri-full
test-miri: miri-quick

miri-quick:
	@printf "$(YELLOW)Running quick Miri memory safety tests...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --quick
	@printf "$(GREEN)Quick Miri tests completed$(RESET)\n"

miri-core:
	@printf "$(YELLOW)Running core library Miri tests...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --core-only --verbose
	@printf "$(GREEN)Core Miri tests completed$(RESET)\n"

miri-proptest:
	@printf "$(YELLOW)Running property-based Miri tests...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --proptest-only --verbose
	@printf "$(GREEN)Property-based Miri tests completed$(RESET)\n"

miri-full:
	@printf "$(YELLOW)Running full Miri test suite (slow)...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	@chmod +x scripts/miri-test.sh
	./scripts/miri-test.sh --verbose
	@printf "$(GREEN)Full Miri test suite completed$(RESET)\n"

# ============================================================
# Individual binary builds
# ============================================================

.PHONY: build-binaries
build-binaries:
	@printf "$(YELLOW)Building all binaries ($(NPROC) jobs)...$(RESET)\n"
	cargo build --release -j $(NPROC) --bin rustmq-broker --bin rustmq-controller --bin rustmq-admin --bin rustmq-bigquery-subscriber --bin rustmq-admin-server $(FEATURES)
	@printf "$(GREEN)All binaries built$(RESET)\n"

# ============================================================
# SDK build targets
# ============================================================

.PHONY: sdk-build sdk-build-debug sdk-build-release
sdk-build: sdk-build-debug sdk-build-release

sdk-build-debug: sdk-rust-build-debug sdk-go-build

sdk-build-release: sdk-rust-build-release sdk-go-build

.PHONY: sdk-rust-build sdk-rust-build-debug sdk-rust-build-release
sdk-rust-build: sdk-rust-build-debug sdk-rust-build-release

sdk-rust-build-debug:
	@printf "$(YELLOW)Building Rust SDK debug mode...$(RESET)\n"
	cd sdk/rust && cargo build -j $(NPROC)
	@printf "$(GREEN)Rust SDK debug build completed$(RESET)\n"

sdk-rust-build-release:
	@printf "$(YELLOW)Building Rust SDK release mode...$(RESET)\n"
	cd sdk/rust && cargo build --release -j $(NPROC)
	@printf "$(GREEN)Rust SDK release build completed$(RESET)\n"

.PHONY: sdk-go-build
sdk-go-build:
	@printf "$(YELLOW)Building Go SDK...$(RESET)\n"
	cd sdk/go && go build ./rustmq/...
	@printf "$(GREEN)Go SDK build completed$(RESET)\n"

# ============================================================
# Code quality checks
# ============================================================

.PHONY: check lint fmt clippy
check:
	@printf "$(YELLOW)Running cargo check...$(RESET)\n"
	cargo check -j $(NPROC) $(FEATURES)
	@printf "$(GREEN)Check completed$(RESET)\n"

lint: clippy fmt sdk-go-fmt sdk-go-vet

clippy:
	@printf "$(YELLOW)Running clippy...$(RESET)\n"
	cargo clippy -j $(NPROC) --all-targets $(FEATURES) -- -D warnings
	@printf "$(GREEN)Clippy completed$(RESET)\n"

fmt:
	@printf "$(YELLOW)Running rustfmt check...$(RESET)\n"
	cargo fmt --check
	@printf "$(GREEN)Format check completed$(RESET)\n"

# ============================================================
# Clean targets
# ============================================================

.PHONY: clean clean-all
clean:
	@printf "$(YELLOW)Cleaning build artifacts...$(RESET)\n"
	cargo clean
	@printf "$(GREEN)Clean completed$(RESET)\n"

clean-all: clean
	@printf "$(YELLOW)Removing target directory...$(RESET)\n"
	rm -rf target/
	@printf "$(GREEN)Deep clean completed$(RESET)\n"

# ============================================================
# Benchmark targets — always sequential execution for accuracy
# Compilation uses all cores, but benchmark runs are isolated
# ============================================================

.PHONY: bench bench-cache bench-security bench-wal bench-replication
bench:
	@printf "$(YELLOW)Running all benchmarks (compile parallel, run sequential)...$(RESET)\n"
	cargo bench -j $(NPROC) $(FEATURES)

bench-cache:
	@printf "$(YELLOW)Running cache benchmarks...$(RESET)\n"
	cargo bench -j $(NPROC) --bench cache_performance_bench $(FEATURES)

bench-security:
	@printf "$(YELLOW)Running security benchmarks...$(RESET)\n"
	cargo bench -j $(NPROC) --bench security_performance $(FEATURES)
	cargo bench -j $(NPROC) --bench authorization_benchmarks $(FEATURES)

bench-wal:
	@printf "$(YELLOW)Running WAL benchmarks...$(RESET)\n"
	cargo bench -j $(NPROC) --bench wal_performance_bench $(FEATURES)

bench-replication:
	@printf "$(YELLOW)Running replication benchmarks...$(RESET)\n"
	cargo bench -j $(NPROC) --bench replication_manager_benchmarks $(FEATURES)

# Legacy LRU cache testing (without default features)
.PHONY: test-legacy-cache
test-legacy-cache:
	@printf "$(YELLOW)Testing with legacy LRU cache...$(RESET)\n"
	cargo test -j $(NPROC) --lib $(FEATURES_NO_DEFAULT) -- --test-threads=$(TEST_THREADS)
	@printf "$(GREEN)Legacy cache tests completed$(RESET)\n"

# ============================================================
# SDK test targets
# ============================================================

.PHONY: sdk-test sdk-test-debug sdk-test-release sdk-quick-test
sdk-test: sdk-test-debug sdk-test-release

sdk-test-debug: sdk-rust-test-debug sdk-go-test-debug

sdk-test-release: sdk-rust-test-release sdk-go-test-release

sdk-quick-test:
	@printf "$(YELLOW)Quick SDK verification (library tests only)...$(RESET)\n"
	cd sdk/rust && cargo test -j $(NPROC) --lib
	cd sdk/go && go test -parallel $(TEST_THREADS) ./rustmq/...
	@printf "$(GREEN)Quick SDK tests completed$(RESET)\n"

# Rust SDK test targets
.PHONY: sdk-rust-test sdk-rust-test-debug sdk-rust-test-release sdk-rust-bench sdk-rust-miri
sdk-rust-test: sdk-rust-test-debug sdk-rust-test-release

sdk-rust-test-debug:
	@printf "$(YELLOW)Running Rust SDK debug tests...$(RESET)\n"
	cd sdk/rust && cargo test -j $(NPROC) --lib
	@printf "$(GREEN)Rust SDK debug tests completed$(RESET)\n"

sdk-rust-test-release:
	@printf "$(YELLOW)Running Rust SDK release tests...$(RESET)\n"
	cd sdk/rust && cargo test -j $(NPROC) --lib --release
	@printf "$(YELLOW)Running Rust SDK benchmarks...$(RESET)\n"
	cd sdk/rust && cargo bench -j $(NPROC) || printf "$(YELLOW)Some benchmarks may fail due to missing certificates$(RESET)\n"
	@printf "$(GREEN)Rust SDK release tests completed$(RESET)\n"

sdk-rust-bench:
	@printf "$(YELLOW)Running Rust SDK benchmarks only...$(RESET)\n"
	cd sdk/rust && cargo bench -j $(NPROC)
	@printf "$(GREEN)Rust SDK benchmarks completed$(RESET)\n"

# Rust SDK Miri memory safety tests
.PHONY: sdk-rust-miri sdk-rust-miri-quick sdk-rust-miri-full
sdk-rust-miri: sdk-rust-miri-quick

sdk-rust-miri-quick:
	@printf "$(YELLOW)Running Rust SDK Miri memory safety tests (quick)...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri::client_tests::test_message_memory_safety || printf "$(YELLOW)Some SDK Miri tests may be filtered out$(RESET)\n"
	@printf "$(GREEN)Rust SDK quick Miri tests completed$(RESET)\n"

sdk-rust-miri-full:
	@printf "$(YELLOW)Running full Rust SDK Miri test suite...$(RESET)\n"
	@rustup +nightly component list --installed | grep -q miri || (rustup +nightly component add miri && cargo +nightly miri setup)
	cd sdk/rust && cargo +nightly miri test --features miri-safe miri:: || printf "$(YELLOW)Some SDK Miri tests may be filtered out$(RESET)\n"
	@printf "$(GREEN)Full Rust SDK Miri tests completed$(RESET)\n"

# Go SDK test targets
.PHONY: sdk-go-test sdk-go-test-debug sdk-go-test-release sdk-go-bench
sdk-go-test: sdk-go-test-debug sdk-go-test-release

sdk-go-test-debug:
	@printf "$(YELLOW)Running Go SDK debug tests...$(RESET)\n"
	cd sdk/go && go test -parallel $(TEST_THREADS) ./rustmq/... ./tests/... ./benchmarks/...
	@printf "$(GREEN)Go SDK debug tests completed$(RESET)\n"

sdk-go-test-release:
	@printf "$(YELLOW)Running Go SDK release tests...$(RESET)\n"
	cd sdk/go && go test -parallel $(TEST_THREADS) -ldflags="-s -w" ./rustmq/... ./tests/... ./benchmarks/...
	@printf "$(YELLOW)Running Go SDK benchmarks (release optimized)...$(RESET)\n"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq/... ./tests/... ./benchmarks/...
	@printf "$(GREEN)Go SDK release tests and benchmarks completed$(RESET)\n"

sdk-go-bench:
	@printf "$(YELLOW)Running Go SDK benchmarks (release optimized)...$(RESET)\n"
	cd sdk/go && go test -bench=. -benchmem -ldflags="-s -w" ./rustmq/... ./tests/... ./benchmarks/...
	@printf "$(GREEN)Go SDK benchmarks completed$(RESET)\n"

sdk-go-examples:
	@printf "$(YELLOW)Building Go SDK examples...$(RESET)\n"
	cd sdk/go && for f in examples/*.go; do go build -o bin/$$(basename $$f .go) $$f; done
	@printf "$(GREEN)Go SDK examples built$(RESET)\n"

# Go SDK quality checks
.PHONY: sdk-go-fmt sdk-go-vet sdk-go-mod-tidy
sdk-go-fmt:
	@printf "$(YELLOW)Running go fmt on Go SDK...$(RESET)\n"
	cd sdk/go && go fmt ./...
	@printf "$(GREEN)Go SDK format check completed$(RESET)\n"

sdk-go-vet:
	@printf "$(YELLOW)Running go vet on Go SDK...$(RESET)\n"
	cd sdk/go && go vet ./rustmq/...
	@printf "$(GREEN)Go SDK vet check completed$(RESET)\n"

sdk-go-mod-tidy:
	@printf "$(YELLOW)Running go mod tidy on Go SDK...$(RESET)\n"
	cd sdk/go && go mod tidy
	@printf "$(GREEN)Go SDK mod tidy completed$(RESET)\n"

# ============================================================
# Development helpers
# ============================================================

.PHONY: dev dev-broker dev-controller dev-admin
dev:
	@printf "$(BLUE)Available development commands:$(RESET)\n"
	@echo "  make dev-broker     - Run broker in development mode"
	@echo "  make dev-controller - Run controller in development mode"
	@echo "  make dev-admin      - Run admin CLI"

dev-broker:
	cargo run --bin rustmq-broker $(FEATURES) -- --config config/broker.toml

dev-controller:
	cargo run --bin rustmq-controller $(FEATURES) -- --config config/controller.toml

dev-admin:
	cargo run --bin rustmq-admin $(FEATURES) -- --help

# Docker helpers
.PHONY: docker-build docker-up docker-down
docker-build:
	docker-compose build

docker-up:
	docker-compose up -d

docker-down:
	docker-compose down

# ============================================================
# Help
# ============================================================

.PHONY: help
help:
	@printf "$(BLUE)RustMQ Makefile$(RESET) ($(NPROC) cores detected)\n\n"
	@printf "$(YELLOW)Primary:$(RESET)\n"
	@echo "  all              Build and test everything"
	@echo "  sanity           Pre-commit check (parallel stages + benchmarks)"
	@echo "  info             Show platform and feature detection"
	@echo ""
	@printf "$(YELLOW)Build:$(RESET)\n"
	@echo "  build            Build debug and release"
	@echo "  build-debug      Debug build only"
	@echo "  build-release    Release build only"
	@echo "  build-binaries   Build all release binaries (single cargo invocation)"
	@echo ""
	@printf "$(YELLOW)Test:$(RESET)\n"
	@echo "  test             Debug + release tests"
	@echo "  test-debug       Debug tests"
	@echo "  test-release     Release tests"
	@echo "  test-wasm        WASM integration tests"
	@echo "  test-miri        Quick Miri memory safety tests"
	@echo ""
	@printf "$(YELLOW)SDK:$(RESET)\n"
	@echo "  sdk-test         All SDK tests (Rust + Go)"
	@echo "  sdk-quick-test   Quick SDK library tests only"
	@echo "  sdk-go-examples  Build Go SDK examples"
	@echo ""
	@printf "$(YELLOW)Benchmarks (sequential for accuracy):$(RESET)\n"
	@echo "  bench            All benchmarks"
	@echo "  bench-cache      Cache benchmarks"
	@echo "  bench-security   Security benchmarks"
	@echo "  bench-wal        WAL benchmarks"
	@echo "  bench-replication Replication benchmarks"
	@echo ""
	@printf "$(YELLOW)Quality:$(RESET)\n"
	@echo "  check            cargo check"
	@echo "  lint             clippy + fmt + go checks"
	@echo "  fmt              Format check"
	@echo ""
	@printf "$(GREEN)Platform: $(PLATFORM_INFO)$(RESET)\n"
	@printf "$(GREEN)Features: $(FEATURES)$(RESET)\n"
