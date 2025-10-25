# WASM Test Modules

This directory contains WebAssembly test modules for validating the ETL WASM integration.

## Modules

### simple_transform
- **Purpose**: Basic transformation testing
- **Function**: Converts text to uppercase
- **Size**: ~462 bytes
- **Use**: Validates basic WASM execution and memory protocol

### infinite_loop
- **Purpose**: Timeout enforcement testing
- **Function**: Infinite loop that never returns
- **Size**: ~208 bytes
- **Use**: Validates timeout mechanisms and fuel exhaustion

### fuel_exhaustion
- **Purpose**: CPU limit testing
- **Function**: Expensive recursive fibonacci computation
- **Size**: ~371 bytes
- **Use**: Validates fuel metering limits CPU usage

## Building

### Quick Build (All Modules)
```bash
./build-all.sh
```

### Manual Build (Individual Modules)
```bash
cd simple_transform
cargo build --target wasm32-unknown-unknown --release

# Output: target/wasm32-unknown-unknown/release/simple_transform.wasm
```

### Requirements
- Rust toolchain with wasm32-unknown-unknown target
- Install if needed: `rustup target add wasm32-unknown-unknown`

## Output Location

Compiled WASM binaries are placed in:
```
tests/wasm_binaries/
├── simple_transform.wasm
├── infinite_loop.wasm
└── fuel_exhaustion.wasm
```

**Note**: This directory is in `.gitignore` - binaries are not checked into Git.

## Memory Protocol

All modules follow this protocol:

1. **allocate(size: i32) -> i32** - Allocate memory, returns pointer
2. **transform(input_ptr: i32, input_len: i32) -> i32** - Process data, returns output pointer
3. **deallocate(ptr: i32)** - Free memory (no-op for bump allocator)

### Output Format
```
[4 bytes: length (little-endian i32)][N bytes: data]
```

## Testing

Run integration tests with real WASM execution:
```bash
# Build modules first
./build-all.sh

# Run integration test
cargo test --test wasm_integration_simple --features wasm -- --nocapture
```

## Architecture

All modules are `no_std` with:
- **No heap allocator**: Uses simple bump allocator
- **No panic unwinding**: Custom panic handler loops forever
- **Minimal size**: Optimized for size with `opt-level = "z"`, LTO, and stripping

## Maintenance

When adding new test modules:
1. Create directory: `tests/wasm_modules/new_module/`
2. Add `Cargo.toml` with:
   ```toml
   [workspace]  # Prevent parent workspace detection

   [lib]
   crate-type = ["cdylib"]

   [profile.release]
   opt-level = "z"
   lto = true
   strip = true
   ```
3. Create `src/lib.rs` with `#![no_std]` and panic handler
4. Update `build-all.sh` to include new module
5. Build and test!
