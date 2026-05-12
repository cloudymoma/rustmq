//! Complex integration tests for WASM edge cases (fuel exhaustion, infinite loop)
//! This validates that resource limits are correctly enforced.

#[cfg(feature = "wasm")]
#[tokio::test]
async fn test_wasm_fuel_exhaustion() {
    use rustmq::config::{EtlInstancePoolConfig, ModuleInstanceConfig};
    use rustmq::etl::instance_pool::WasmInstancePool;
    use serde_json::json;
    use std::sync::Arc;

    // Load fuel exhaustion WASM bytecode
    let wasm_path = "tests/wasm_binaries/fuel_exhaustion.wasm";
    let bytecode = std::fs::read(wasm_path).expect("Failed to load WASM binary");

    // Create pool
    let pool_config = EtlInstancePoolConfig {
        max_pool_size: 5,
        warmup_instances: 0,
        creation_rate_limit: 100.0,
        idle_timeout_seconds: 300,
        enable_lru_eviction: true,
    };

    let pool = Arc::new(WasmInstancePool::new(pool_config).unwrap());

    // Load module
    pool.load_module("fuel_exhaustion".to_string(), bytecode)
        .await
        .expect("Failed to load WASM module");

    // Create module config with LOW timeout to result in low fuel
    let module_config = ModuleInstanceConfig {
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 1, // 1 ms timeout -> ~1M instructions
        max_concurrent_instances: 10,
        enable_caching: false,
        cache_ttl_seconds: 0,
        custom_config: json!({}),
    };

    // Checkout instance
    let mut checkout = pool
        .checkout_instance("fuel_exhaustion", &module_config)
        .await
        .expect("Failed to checkout instance");

    // Execute WASM transformation with input that triggers heavy computation
    // Value 30 results in fibonacci(30) which is very expensive
    let input = &[30u8];
    let result = checkout
        .instance
        .wasm_context
        .execute("transform", input)
        .await;

    // We expect failure due to fuel exhaustion
    assert!(
        result.is_err(),
        "WASM execution should have failed due to fuel exhaustion"
    );

    let remaining = checkout
        .instance
        .wasm_context
        .remaining_fuel()
        .unwrap_or(999);
    assert_eq!(
        remaining, 0,
        "Remaining fuel should be 0 on exhaustion, got: {}",
        remaining
    );

    println!("✅ Fuel exhaustion test passed - module was correctly terminated.");
}

#[cfg(feature = "wasm")]
#[tokio::test]
async fn test_wasm_infinite_loop() {
    use rustmq::config::{EtlInstancePoolConfig, ModuleInstanceConfig};
    use rustmq::etl::instance_pool::WasmInstancePool;
    use serde_json::json;
    use std::sync::Arc;

    // Load infinite loop WASM bytecode
    let wasm_path = "tests/wasm_binaries/infinite_loop.wasm";
    let bytecode = std::fs::read(wasm_path).expect("Failed to load WASM binary");

    // Create pool
    let pool_config = EtlInstancePoolConfig {
        max_pool_size: 5,
        warmup_instances: 0,
        creation_rate_limit: 100.0,
        idle_timeout_seconds: 300,
        enable_lru_eviction: true,
    };

    let pool = Arc::new(WasmInstancePool::new(pool_config).unwrap());

    // Load module
    pool.load_module("infinite_loop".to_string(), bytecode)
        .await
        .expect("Failed to load WASM module");

    // Create module config with reasonable timeout
    let module_config = ModuleInstanceConfig {
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 50, // 50 ms timeout -> ~50M instructions
        max_concurrent_instances: 10,
        enable_caching: false,
        cache_ttl_seconds: 0,
        custom_config: json!({}),
    };

    // Checkout instance
    let mut checkout = pool
        .checkout_instance("infinite_loop", &module_config)
        .await
        .expect("Failed to checkout instance");

    // Execute WASM transformation
    let input = b"test input";
    let result = checkout
        .instance
        .wasm_context
        .execute("transform", input)
        .await;

    // We expect failure due to fuel exhaustion (simulating timeout)
    assert!(
        result.is_err(),
        "WASM execution should have failed due to infinite loop"
    );

    let remaining = checkout
        .instance
        .wasm_context
        .remaining_fuel()
        .unwrap_or(999);
    assert_eq!(
        remaining, 0,
        "Remaining fuel should be 0 on exhaustion, got: {}",
        remaining
    );

    println!("✅ Infinite loop test passed - module was correctly terminated.");
}
