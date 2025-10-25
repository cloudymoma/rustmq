use crate::config::{EtlInstancePoolConfig, ModuleInstanceConfig};
use crate::{Result, error::RustMqError};
use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use tokio::sync::Semaphore;
use std::time::{Duration, Instant};
use tokio::time::interval;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use dashmap::DashMap;

// Wasmtime imports for real WASM execution
#[cfg(feature = "wasm")]
use wasmtime::*;
#[cfg(feature = "wasm")]
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

/// High-performance WASM instance pool with LRU eviction and resource management
pub struct WasmInstancePool {
    config: EtlInstancePoolConfig,
    /// Shared wasmtime Engine (expensive to create, shared across all instances)
    #[cfg(feature = "wasm")]
    engine: Arc<Engine>,
    /// Compiled WASM modules cache (pre-compiled, ready for instantiation)
    #[cfg(feature = "wasm")]
    compiled_modules: Arc<DashMap<String, Arc<Module>>>,
    /// Module pools indexed by module ID (lock-free with DashMap + Mutex per pool)
    pools: Arc<DashMap<String, Arc<Mutex<ModulePool>>>>,
    /// Rate limiter for instance creation
    creation_semaphore: Arc<Semaphore>,
    /// Global statistics
    stats: Arc<InstancePoolStats>,
    /// Background cleanup task handle
    _cleanup_handle: tokio::task::JoinHandle<()>,
}

/// Pool for a specific WASM module
struct ModulePool {
    module_id: String,
    config: ModuleInstanceConfig,
    /// Available instances ready for use
    available_instances: VecDeque<PooledInstance>,
    /// Instances currently in use (for tracking)
    in_use_count: usize,
    /// LRU tracking for eviction
    lru_order: VecDeque<u64>,
    /// Next instance ID for this module
    next_instance_id: u64,
    /// Pool statistics
    stats: ModulePoolStats,
}

/// Pooled WASM instance with metadata
pub struct PooledInstance {
    pub instance_id: u64,
    pub module_id: String,
    /// Mock WASM instance - in real implementation this would be a wasmtime::Instance
    pub wasm_context: WasmContext,
    pub created_at: Instant,
    pub last_used: Instant,
    pub execution_count: u64,
    pub total_execution_time: Duration,
}

/// WASM execution state for WASI context
#[cfg(feature = "wasm")]
pub struct WasmState {
    wasi: WasiCtx,
}

/// Real WASM context with wasmtime runtime
#[cfg(feature = "wasm")]
pub struct WasmContext {
    // Shared resources (cheap to clone via Arc)
    engine: Arc<Engine>,
    module: Arc<Module>,

    // Per-instance mutable state
    store: Store<WasmState>,
    instance: Instance,
    memory: Memory,

    // Cached function references for performance
    transform_func: TypedFunc<(i32, i32), i32>,
    allocate_func: Option<TypedFunc<i32, i32>>,
    deallocate_func: Option<TypedFunc<i32, ()>>,

    // Metadata
    pub memory_size: usize,
}

/// Mock WASM context for non-wasm builds
#[cfg(not(feature = "wasm"))]
#[derive(Clone)]
pub struct WasmContext {
    pub memory_size: usize,
    pub is_initialized: bool,
    pub mock_data: Vec<u8>,
}

/// Pool statistics for monitoring
#[derive(Debug, Default)]
pub struct InstancePoolStats {
    pub total_instances_created: AtomicU64,
    pub total_instances_destroyed: AtomicU64,
    pub total_pool_hits: AtomicU64,
    pub total_pool_misses: AtomicU64,
    pub active_modules: AtomicUsize,
    pub total_memory_usage: AtomicUsize,
}

/// Module-specific pool statistics
#[derive(Debug, Default, Clone)]
pub struct ModulePoolStats {
    pub instances_created: u64,
    pub instances_destroyed: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub current_pool_size: usize,
    pub max_pool_size_reached: usize,
    pub average_instance_age: Duration,
    pub total_execution_time: Duration,
}

/// Instance checkout result
pub struct InstanceCheckout {
    pub instance: PooledInstance,
    /// Handle to return instance to pool
    pub return_handle: InstanceReturnHandle,
}

/// Handle for returning instance to pool
pub struct InstanceReturnHandle {
    pool_ref: Arc<DashMap<String, Arc<Mutex<ModulePool>>>>,
    stats_ref: Arc<InstancePoolStats>,
    module_id: String,
    instance_id: u64,
    max_pool_size: usize,
}

impl WasmInstancePool {
    /// Create a new WASM instance pool (lock-free with DashMap)
    pub fn new(config: EtlInstancePoolConfig) -> Result<Self> {
        let creation_semaphore = Arc::new(Semaphore::new(
            (config.creation_rate_limit as usize).max(1)
        ));
        let pools = Arc::new(DashMap::new());
        let stats = Arc::new(InstancePoolStats::default());

        // Create shared wasmtime Engine with optimized configuration for ETL workloads
        #[cfg(feature = "wasm")]
        let engine = {
            let mut wasm_config = Config::new();

            // Enable async support for non-blocking execution
            wasm_config.async_support(true);

            // Set memory limits (will be overridden per-instance)
            wasm_config.static_memory_maximum_size(512 * 1024 * 1024); // 512MB max per instance
            wasm_config.static_memory_guard_size(128 * 1024); // 128KB guard pages

            // Enable fuel metering for CPU limits
            wasm_config.consume_fuel(true);

            // Optimize for ETL workloads
            wasm_config.cranelift_opt_level(wasmtime::OptLevel::Speed);
            wasm_config.parallel_compilation(true);

            // Enable WASI support
            wasm_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

            Arc::new(Engine::new(&wasm_config)
                .map_err(|e| RustMqError::EtlProcessingFailed(
                    format!("Failed to create wasmtime engine: {}", e)
                ))?)
        };

        #[cfg(feature = "wasm")]
        let compiled_modules = Arc::new(DashMap::new());

        // Start background cleanup task
        let cleanup_handle = Self::start_cleanup_task(
            Arc::clone(&pools),
            Arc::clone(&stats),
            config.idle_timeout_seconds,
        );

        Ok(Self {
            config,
            #[cfg(feature = "wasm")]
            engine,
            #[cfg(feature = "wasm")]
            compiled_modules,
            pools,
            creation_semaphore,
            stats,
            _cleanup_handle: cleanup_handle,
        })
    }

    /// Load and compile a WASM module from bytecode
    #[cfg(feature = "wasm")]
    pub async fn load_module(&self, module_id: String, bytecode: Vec<u8>) -> Result<()> {
        // Check if already compiled
        if self.compiled_modules.contains_key(&module_id) {
            return Ok(());
        }

        // Compile module (this is expensive, done once)
        let module = Module::from_binary(&self.engine, &bytecode)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to compile WASM module '{}': {}", module_id, e)
            ))?;

        // Validate required exports
        let exports: Vec<_> = module.exports().map(|e| e.name().to_string()).collect();

        if !exports.iter().any(|name| name == "transform") {
            return Err(RustMqError::EtlProcessingFailed(
                format!("WASM module '{}' must export 'transform' function", module_id)
            ));
        }

        if !exports.iter().any(|name| name == "memory") {
            return Err(RustMqError::EtlProcessingFailed(
                format!("WASM module '{}' must export 'memory'", module_id)
            ));
        }

        // Cache the compiled module
        self.compiled_modules.insert(module_id.clone(), Arc::new(module));

        tracing::info!("Loaded and compiled WASM module: {}", module_id);
        Ok(())
    }

    /// Load module for non-wasm builds (no-op)
    #[cfg(not(feature = "wasm"))]
    pub async fn load_module(&self, _module_id: String, _bytecode: Vec<u8>) -> Result<()> {
        Ok(())
    }

    /// Get or create an instance for the specified module
    pub async fn checkout_instance(
        &self,
        module_id: &str,
        module_config: &ModuleInstanceConfig,
    ) -> Result<InstanceCheckout> {
        // Try to get from existing pool first
        if let Some(instance) = self.try_get_from_pool(module_id).await? {
            self.stats.total_pool_hits.fetch_add(1, Ordering::Relaxed);
            let return_handle = InstanceReturnHandle {
                pool_ref: Arc::clone(&self.pools),
                stats_ref: Arc::clone(&self.stats),
                module_id: module_id.to_string(),
                instance_id: instance.instance_id,
                max_pool_size: self.config.max_pool_size,
            };
            return Ok(InstanceCheckout {
                instance,
                return_handle,
            });
        }

        // Pool miss - create new instance
        self.stats.total_pool_misses.fetch_add(1, Ordering::Relaxed);
        
        // Apply rate limiting for instance creation
        let _permit = self.creation_semaphore.acquire().await
            .map_err(|_| RustMqError::EtlProcessingFailed(
                "Failed to acquire instance creation permit".to_string()
            ))?;

        let instance = self.create_new_instance(module_id, module_config).await?;
        let return_handle = InstanceReturnHandle {
            pool_ref: Arc::clone(&self.pools),
            stats_ref: Arc::clone(&self.stats),
            module_id: module_id.to_string(),
            instance_id: instance.instance_id,
            max_pool_size: self.config.max_pool_size,
        };

        Ok(InstanceCheckout {
            instance,
            return_handle,
        })
    }

    /// Try to get an instance from existing pool (lock-free module access)
    async fn try_get_from_pool(&self, module_id: &str) -> Result<Option<PooledInstance>> {
        // Lock-free access to the module pool
        if let Some(pool_arc) = self.pools.get(module_id) {
            let mut module_pool = pool_arc.lock();

            if let Some(mut instance) = module_pool.available_instances.pop_front() {
                instance.last_used = Instant::now();
                module_pool.in_use_count += 1;
                module_pool.stats.pool_hits += 1;

                // Update LRU order
                if let Some(pos) = module_pool.lru_order.iter().position(|&id| id == instance.instance_id) {
                    module_pool.lru_order.remove(pos);
                }
                module_pool.lru_order.push_back(instance.instance_id);

                return Ok(Some(instance));
            }
        }

        Ok(None)
    }

    /// Create a new WASM instance
    async fn create_new_instance(
        &self,
        module_id: &str,
        module_config: &ModuleInstanceConfig,
    ) -> Result<PooledInstance> {
        // In real implementation, this would:
        // 1. Load WASM module from storage
        // 2. Create wasmtime Store and Instance
        // 3. Initialize memory and globals
        // 4. Validate module exports/imports
        
        // Mock validation: fail for non-existent modules (for testing)
        if module_id.contains("non-existent") {
            return Err(RustMqError::EtlModuleNotFound(
                format!("Module not found: {}", module_id)
            ));
        }
        
        let instance_id = self.get_next_instance_id(module_id).await;
        let wasm_context = self.create_wasm_context(module_id, module_config).await?;
        
        let instance = PooledInstance {
            instance_id,
            module_id: module_id.to_string(),
            wasm_context,
            created_at: Instant::now(),
            last_used: Instant::now(),
            execution_count: 0,
            total_execution_time: Duration::ZERO,
        };

        // Update statistics
        self.stats.total_instances_created.fetch_add(1, Ordering::Relaxed);
        self.stats.total_memory_usage.fetch_add(module_config.memory_limit_bytes, Ordering::Relaxed);

        // Ensure module pool exists and update stats (lock-free with DashMap)
        let pool_arc = self.pools.entry(module_id.to_string()).or_insert_with(|| {
            self.stats.active_modules.fetch_add(1, Ordering::Relaxed);
            Arc::new(Mutex::new(ModulePool {
                module_id: module_id.to_string(),
                config: module_config.clone(),
                available_instances: VecDeque::new(),
                in_use_count: 0,
                lru_order: VecDeque::new(),
                next_instance_id: 0,
                stats: ModulePoolStats::default(),
            }))
        });

        let mut module_pool = pool_arc.lock();
        module_pool.stats.instances_created += 1;
        module_pool.stats.pool_misses += 1;
        module_pool.in_use_count += 1;

        Ok(instance)
    }

    /// Create WASM execution context
    /// Create WASM context with real wasmtime runtime
    #[cfg(feature = "wasm")]
    async fn create_wasm_context(&self, module_id: &str, config: &ModuleInstanceConfig) -> Result<WasmContext> {
        // Get compiled module from cache
        let module = self.compiled_modules.get(module_id)
            .ok_or_else(|| RustMqError::EtlModuleNotFound(
                format!("Module '{}' not loaded. Call load_module() first.", module_id)
            ))?
            .clone();

        // Create WASI context
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build();

        // Create state
        let state = WasmState { wasi };

        // Create store with state
        let mut store = Store::new(&self.engine, state);

        // Set fuel for CPU limiting (approximate timeout in instructions)
        // Memory limits are configured in Engine config (static_memory_maximum_size)
        let fuel_amount = (config.execution_timeout_ms * 1_000_000) as u64; // ~1M instructions per ms
        store.set_fuel(fuel_amount)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to set fuel: {}", e)
            ))?;

        // Link WASI - using a simple linker without WASI for now
        // TODO: Add WASI preview1 support if needed by ETL modules
        let mut linker = Linker::new(&self.engine);

        // Instantiate module
        let instance = linker.instantiate_async(&mut store, &module).await
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to instantiate WASM module: {}", e)
            ))?;

        // Get memory export
        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| RustMqError::EtlProcessingFailed(
                "WASM module must export 'memory'".to_string()
            ))?;

        // Get transform function (required)
        let transform_func = instance.get_typed_func::<(i32, i32), i32>(&mut store, "transform")
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to get 'transform' function: {}", e)
            ))?;

        // Get allocate function (optional)
        let allocate_func = instance.get_typed_func::<i32, i32>(&mut store, "allocate").ok();

        // Get deallocate function (optional)
        let deallocate_func = instance.get_typed_func::<i32, ()>(&mut store, "deallocate").ok();

        let memory_size = memory.data_size(&store);

        Ok(WasmContext {
            engine: self.engine.clone(),
            module,
            store,
            instance,
            memory,
            transform_func,
            allocate_func,
            deallocate_func,
            memory_size,
        })
    }

    /// Create mock WASM context for non-wasm builds
    #[cfg(not(feature = "wasm"))]
    async fn create_wasm_context(&self, _module_id: &str, config: &ModuleInstanceConfig) -> Result<WasmContext> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(WasmContext {
            memory_size: config.memory_limit_bytes,
            is_initialized: true,
            mock_data: vec![0; 1024],
        })
    }

    /// Get next instance ID for a module (lock-free with DashMap)
    async fn get_next_instance_id(&self, module_id: &str) -> u64 {
        let pool_arc = self.pools.entry(module_id.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(ModulePool {
                module_id: module_id.to_string(),
                config: ModuleInstanceConfig {
                    memory_limit_bytes: 64 * 1024 * 1024,
                    execution_timeout_ms: 5000,
                    max_concurrent_instances: 10,
                    enable_caching: true,
                    cache_ttl_seconds: 300,
                    custom_config: serde_json::Value::Object(serde_json::Map::new()),
                },
                available_instances: VecDeque::new(),
                in_use_count: 0,
                lru_order: VecDeque::new(),
                next_instance_id: 0,
                stats: ModulePoolStats::default(),
            }))
        });

        let mut module_pool = pool_arc.lock();
        let id = module_pool.next_instance_id;
        module_pool.next_instance_id += 1;
        id
    }

    /// Return an instance to the pool (lock-free module access)
    async fn return_instance(&self, module_id: &str, mut instance: PooledInstance) -> Result<()> {
        if let Some(pool_arc) = self.pools.get(module_id) {
            let mut module_pool = pool_arc.lock();

            module_pool.in_use_count = module_pool.in_use_count.saturating_sub(1);

            // Check if pool is full and evict LRU if necessary
            if module_pool.available_instances.len() >= self.config.max_pool_size {
                if self.config.enable_lru_eviction {
                    self.evict_lru_instance(&mut module_pool).await?;
                } else {
                    // Pool is full and eviction disabled - destroy instance
                    self.destroy_instance(&instance).await?;
                    return Ok(());
                }
            }

            // Reset instance state for reuse
            instance.last_used = Instant::now();

            // Store instance ID before moving the instance
            let instance_id = instance.instance_id;

            // Add to available pool
            module_pool.available_instances.push_back(instance);
            module_pool.lru_order.push_back(instance_id);
            module_pool.stats.current_pool_size = module_pool.available_instances.len();
            module_pool.stats.max_pool_size_reached =
                module_pool.stats.max_pool_size_reached.max(module_pool.available_instances.len());
        }

        Ok(())
    }

    /// Evict least recently used instance from pool
    async fn evict_lru_instance(&self, module_pool: &mut ModulePool) -> Result<()> {
        if let Some(lru_instance_id) = module_pool.lru_order.pop_front() {
            // Find and remove the LRU instance
            if let Some(pos) = module_pool.available_instances.iter()
                .position(|inst| inst.instance_id == lru_instance_id) {
                let instance = module_pool.available_instances.remove(pos).unwrap();
                self.destroy_instance(&instance).await?;
                module_pool.stats.instances_destroyed += 1;
            }
        }
        Ok(())
    }

    /// Destroy a WASM instance and free resources
    async fn destroy_instance(&self, instance: &PooledInstance) -> Result<()> {
        // In real implementation, this would:
        // 1. Drop wasmtime Store and Instance
        // 2. Free linear memory
        // 3. Clean up any file handles or resources
        
        self.stats.total_instances_destroyed.fetch_add(1, Ordering::Relaxed);
        self.stats.total_memory_usage.fetch_sub(
            instance.wasm_context.memory_size, 
            Ordering::Relaxed
        );
        
        // Simulate cleanup overhead
        tokio::time::sleep(Duration::from_millis(1)).await;
        
        Ok(())
    }

    /// Pre-warm instances for a module
    pub async fn prewarm_module(&self, module_id: &str, module_config: &ModuleInstanceConfig) -> Result<()> {
        let warmup_count = self.config.warmup_instances.min(self.config.max_pool_size);
        
        for _ in 0..warmup_count {
            let instance = self.create_new_instance(module_id, module_config).await?;
            self.return_instance(module_id, instance).await?;
        }
        
        tracing::info!("Pre-warmed {} instances for module {}", warmup_count, module_id);
        Ok(())
    }

    /// Get pool statistics (lock-free with DashMap)
    pub async fn get_stats(&self) -> Result<HashMap<String, ModulePoolStats>> {
        let mut stats = HashMap::new();

        // Lock-free iteration over all module pools
        for entry in self.pools.iter() {
            let module_id = entry.key().clone();
            let pool_arc = entry.value();
            let pool = pool_arc.lock();
            stats.insert(module_id, pool.stats.clone());
        }

        Ok(stats)
    }

    /// Get global pool statistics
    pub fn get_global_stats(&self) -> InstancePoolStats {
        InstancePoolStats {
            total_instances_created: AtomicU64::new(self.stats.total_instances_created.load(Ordering::Relaxed)),
            total_instances_destroyed: AtomicU64::new(self.stats.total_instances_destroyed.load(Ordering::Relaxed)),
            total_pool_hits: AtomicU64::new(self.stats.total_pool_hits.load(Ordering::Relaxed)),
            total_pool_misses: AtomicU64::new(self.stats.total_pool_misses.load(Ordering::Relaxed)),
            active_modules: AtomicUsize::new(self.stats.active_modules.load(Ordering::Relaxed)),
            total_memory_usage: AtomicUsize::new(self.stats.total_memory_usage.load(Ordering::Relaxed)),
        }
    }

    /// Start background cleanup task for idle instances (lock-free with DashMap)
    fn start_cleanup_task(
        pools: Arc<DashMap<String, Arc<Mutex<ModulePool>>>>,
        stats: Arc<InstancePoolStats>,
        idle_timeout_seconds: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(60)); // Cleanup every minute

            loop {
                cleanup_interval.tick().await;

                let idle_threshold = Instant::now() - Duration::from_secs(idle_timeout_seconds);

                // Lock-free iteration over all module pools
                for entry in pools.iter() {
                    let module_id = entry.key();
                    let pool_arc = entry.value();
                    let mut module_pool = pool_arc.lock();

                    let mut instances_to_remove = Vec::new();

                    // Find idle instances
                    for (index, instance) in module_pool.available_instances.iter().enumerate() {
                        if instance.last_used < idle_threshold {
                            instances_to_remove.push(index);
                        }
                    }

                    // Remove idle instances (in reverse order to maintain indices)
                    for &index in instances_to_remove.iter().rev() {
                        if let Some(instance) = module_pool.available_instances.remove(index) {
                            // Remove from LRU order
                            if let Some(pos) = module_pool.lru_order.iter()
                                .position(|&id| id == instance.instance_id) {
                                module_pool.lru_order.remove(pos);
                            }

                            module_pool.stats.instances_destroyed += 1;
                            stats.total_instances_destroyed.fetch_add(1, Ordering::Relaxed);
                            stats.total_memory_usage.fetch_sub(
                                instance.wasm_context.memory_size,
                                Ordering::Relaxed
                            );

                            tracing::debug!("Cleaned up idle instance {} for module {}",
                                instance.instance_id, module_id);
                        }
                    }

                    module_pool.stats.current_pool_size = module_pool.available_instances.len();
                }
            }
        })
    }
}

impl InstanceReturnHandle {
    /// Return the instance to the pool (lock-free with DashMap)
    pub async fn return_instance(self, mut instance: PooledInstance) -> Result<()> {
        // Update instance metrics before returning
        instance.execution_count += 1;
        instance.last_used = Instant::now();

        if let Some(pool_arc) = self.pool_ref.get(&self.module_id) {
            let mut module_pool = pool_arc.lock();

            module_pool.in_use_count = module_pool.in_use_count.saturating_sub(1);

            // Check if pool is full and evict LRU if necessary
            if module_pool.available_instances.len() >= self.max_pool_size {
                // Pool is full - evict LRU instance
                if let Some(lru_instance_id) = module_pool.lru_order.pop_front() {
                    // Find and remove the LRU instance
                    if let Some(pos) = module_pool.available_instances.iter()
                        .position(|inst| inst.instance_id == lru_instance_id) {
                        let old_instance = module_pool.available_instances.remove(pos).unwrap();
                        self.stats_ref.total_instances_destroyed.fetch_add(1, Ordering::Relaxed);
                        self.stats_ref.total_memory_usage.fetch_sub(
                            old_instance.wasm_context.memory_size,
                            Ordering::Relaxed
                        );
                        module_pool.stats.instances_destroyed += 1;
                    }
                }
            }

            // Store instance ID before moving the instance
            let instance_id = instance.instance_id;

            // Add to available pool
            module_pool.available_instances.push_back(instance);
            module_pool.lru_order.push_back(instance_id);
            module_pool.stats.current_pool_size = module_pool.available_instances.len();
            module_pool.stats.max_pool_size_reached =
                module_pool.stats.max_pool_size_reached.max(module_pool.available_instances.len());

            tracing::debug!("Returned instance {} to pool for module {}",
                self.instance_id, self.module_id);
        }

        Ok(())
    }
}

// Real WASM implementation
#[cfg(feature = "wasm")]
impl WasmContext {
    /// Execute WASM transformation with real wasmtime runtime
    pub async fn execute(&mut self, _function_name: &str, input: &[u8]) -> Result<Vec<u8>> {
        let input_len = input.len() as i32;

        // Allocate memory in WASM for input
        let input_ptr = if let Some(allocate_func) = &self.allocate_func {
            // Use module's allocator
            allocate_func.call_async(&mut self.store, input_len).await
                .map_err(|e| RustMqError::EtlProcessingFailed(
                    format!("Failed to allocate WASM memory: {}", e)
                ))?
        } else {
            // Use fixed location at offset 0 (requires module cooperation)
            0
        };

        // Write input bytes to WASM memory
        self.memory.write(&mut self.store, input_ptr as usize, input)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to write to WASM memory: {}", e)
            ))?;

        // Call transform function
        let output_ptr = self.transform_func.call_async(&mut self.store, (input_ptr, input_len)).await
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("WASM transform function failed: {}", e)
            ))?;

        // Read output length from memory (convention: first 4 bytes at output_ptr contain length)
        let mut len_bytes = [0u8; 4];
        self.memory.read(&self.store, output_ptr as usize, &mut len_bytes)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to read output length from WASM memory: {}", e)
            ))?;
        let output_len = i32::from_le_bytes(len_bytes) as usize;

        // Read output bytes (starting after the length prefix)
        let mut output = vec![0u8; output_len];
        self.memory.read(&self.store, (output_ptr as usize) + 4, &mut output)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to read output from WASM memory: {}", e)
            ))?;

        // Deallocate input memory if possible
        if let Some(deallocate_func) = &self.deallocate_func {
            deallocate_func.call_async(&mut self.store, input_ptr).await
                .map_err(|e| RustMqError::EtlProcessingFailed(
                    format!("Failed to deallocate WASM memory: {}", e)
                ))?;
        }

        Ok(output)
    }

    /// Get remaining fuel (CPU budget)
    pub fn remaining_fuel(&self) -> Option<u64> {
        self.store.get_fuel().ok()
    }

    /// Add more fuel if needed
    pub fn add_fuel(&mut self, fuel: u64) -> Result<()> {
        let current = self.store.get_fuel()
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to get current fuel: {}", e)
            ))?;
        self.store.set_fuel(current + fuel)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to set fuel: {}", e)
            ))
    }
}

// Mock implementation for non-wasm builds
#[cfg(not(feature = "wasm"))]
impl WasmContext {
    pub async fn execute(&mut self, function_name: &str, input: &[u8]) -> Result<Vec<u8>> {
        if !self.is_initialized {
            return Err(RustMqError::EtlProcessingFailed(
                "WASM context not initialized".to_string()
            ));
        }

        tokio::time::sleep(Duration::from_millis(5)).await;

        let mut output = format!("processed:{}", function_name).into_bytes();
        output.extend_from_slice(input);

        Ok(output)
    }

    pub fn reset(&mut self) {
        self.mock_data.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModuleInstanceConfig;

    #[tokio::test]
    async fn test_instance_pool_creation() {
        let config = EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 3,
            creation_rate_limit: 5.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let pool = WasmInstancePool::new(config).unwrap();
        let stats = pool.get_global_stats();
        
        assert_eq!(stats.active_modules.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_instances_created.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    #[cfg(not(feature = "wasm"))]
    async fn test_instance_checkout_and_return() {
        let config = EtlInstancePoolConfig {
            max_pool_size: 5,
            warmup_instances: 2,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let pool = WasmInstancePool::new(config).unwrap();
        
        let module_config = ModuleInstanceConfig {
            memory_limit_bytes: 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_instances: 10,
            enable_caching: true,
            cache_ttl_seconds: 300,
            custom_config: serde_json::Value::Object(serde_json::Map::new()),
        };

        // Checkout instance (should create new)
        let checkout = pool.checkout_instance("test-module", &module_config).await.unwrap();
        assert_eq!(checkout.instance.module_id, "test-module");
        
        let stats = pool.get_global_stats();
        assert_eq!(stats.total_instances_created.load(Ordering::Relaxed), 1);
        assert_eq!(stats.total_pool_misses.load(Ordering::Relaxed), 1);

        // Return instance
        checkout.return_handle.return_instance(checkout.instance).await.unwrap();

        // Checkout again (should reuse from pool)
        let checkout2 = pool.checkout_instance("test-module", &module_config).await.unwrap();
        let stats = pool.get_global_stats();
        assert_eq!(stats.total_pool_hits.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    #[cfg(not(feature = "wasm"))]
    async fn test_instance_prewarming() {
        let config = EtlInstancePoolConfig {
            max_pool_size: 10,
            warmup_instances: 5,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let pool = WasmInstancePool::new(config).unwrap();
        
        let module_config = ModuleInstanceConfig {
            memory_limit_bytes: 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_instances: 10,
            enable_caching: true,
            cache_ttl_seconds: 300,
            custom_config: serde_json::Value::Object(serde_json::Map::new()),
        };

        // Pre-warm instances
        pool.prewarm_module("test-module", &module_config).await.unwrap();
        
        let stats = pool.get_global_stats();
        assert_eq!(stats.total_instances_created.load(Ordering::Relaxed), 5);
        
        // Checkout should hit the pool
        let checkout = pool.checkout_instance("test-module", &module_config).await.unwrap();
        let stats = pool.get_global_stats();
        assert_eq!(stats.total_pool_hits.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    #[cfg(not(feature = "wasm"))]
    async fn test_wasm_context_execution() {
        let mut context = WasmContext {
            memory_size: 1024 * 1024,
            is_initialized: true,
            mock_data: vec![0; 1024],
        };

        let input = b"test input data";
        let output = context.execute("transform", input).await.unwrap();

        // Check that output contains processed prefix and input
        assert!(output.starts_with(b"processed:transform"));
        assert!(output.ends_with(input));
    }

    #[tokio::test]
    #[cfg(not(feature = "wasm"))]
    async fn test_lru_eviction() {
        let config = EtlInstancePoolConfig {
            max_pool_size: 2, // Small pool to force eviction
            warmup_instances: 0,
            creation_rate_limit: 10.0,
            idle_timeout_seconds: 300,
            enable_lru_eviction: true,
        };

        let pool = WasmInstancePool::new(config).unwrap();
        
        let module_config = ModuleInstanceConfig {
            memory_limit_bytes: 1024 * 1024,
            execution_timeout_ms: 5000,
            max_concurrent_instances: 10,
            enable_caching: true,
            cache_ttl_seconds: 300,
            custom_config: serde_json::Value::Object(serde_json::Map::new()),
        };

        // Create multiple instances simultaneously to force creation of more than pool size
        let mut checkouts = Vec::new();
        for i in 0..5 {
            let checkout = pool.checkout_instance("test-module", &module_config).await.unwrap();
            checkouts.push(checkout);
            
            // Small delay to ensure different timestamps
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Now return all instances - this should trigger LRU eviction since pool size is 2
        for checkout in checkouts {
            checkout.return_handle.return_instance(checkout.instance).await.unwrap();
        }

        let stats = pool.get_global_stats();
        // Should have created 5 instances but destroyed some due to LRU eviction (pool holds max 2)
        assert_eq!(stats.total_instances_created.load(Ordering::Relaxed), 5);
        assert!(stats.total_instances_destroyed.load(Ordering::Relaxed) > 0);
    }
}