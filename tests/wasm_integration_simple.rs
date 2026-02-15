//! Simple integration test for real WASM execution
//! This validates that the wasmtime integration actually works with real WASM bytecode

#[cfg(feature = "wasm")]
#[tokio::test]
async fn test_real_wasm_simple_transform_execution() {
    use rustmq::config::{EtlInstancePoolConfig, ModuleInstanceConfig};
    use rustmq::etl::instance_pool::WasmInstancePool;
    use serde_json::json;
    use std::sync::Arc;

    // Load real WASM bytecode
    let wasm_path = "tests/wasm_binaries/simple_transform.wasm";
    let bytecode = std::fs::read(wasm_path)
        .expect("Failed to load WASM. Run: cd tests/wasm_modules/simple_transform && cargo build --target wasm32-unknown-unknown --release");

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
    pool.load_module("simple_transform".to_string(), bytecode)
        .await
        .expect("Failed to load WASM module");

    // Create module config
    let module_config = ModuleInstanceConfig {
        memory_limit_bytes: 64 * 1024 * 1024,
        execution_timeout_ms: 5000,
        max_concurrent_instances: 10,
        enable_caching: false,
        cache_ttl_seconds: 0,
        custom_config: json!({}),
    };

    // Checkout instance
    let mut checkout = pool
        .checkout_instance("simple_transform", &module_config)
        .await
        .expect("Failed to checkout instance");

    // Execute WASM transformation (should convert to uppercase)
    let input = b"hello world";
    let result = checkout
        .instance
        .wasm_context
        .execute("transform", input)
        .await;

    assert!(result.is_ok(), "WASM execution failed: {:?}", result.err());

    let output = result.unwrap();
    assert_eq!(
        output, b"HELLO WORLD",
        "Transformation failed - expected uppercase conversion"
    );

    // Return instance to pool
    checkout
        .return_handle
        .return_instance(checkout.instance)
        .await
        .expect("Failed to return instance");

    println!("✅ Real WASM execution successful - transform function works!");
}

#[cfg(not(feature = "wasm"))]
#[tokio::test]
async fn test_mock_wasm_simple() {
    use rustmq::config::EtlInstancePoolConfig;
    use rustmq::etl::instance_pool::WasmInstancePool;
    use std::sync::Arc;

    let pool_config = EtlInstancePoolConfig {
        max_pool_size: 5,
        warmup_instances: 0,
        creation_rate_limit: 100.0,
        idle_timeout_seconds: 300,
        enable_lru_eviction: true,
    };

    let pool = Arc::new(WasmInstancePool::new(pool_config).unwrap());
    let result = pool.load_module("mock".to_string(), vec![1, 2, 3]).await;
    assert!(result.is_ok(), "Mock load should succeed");

    println!("✅ Mock WASM test passed!");
}
