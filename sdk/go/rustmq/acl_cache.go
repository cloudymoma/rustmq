package rustmq

import (
	"context"
	"fmt"
	"sync"
	"time"
	
	"golang.org/x/sync/singleflight"
)

// ACLCache manages permission caching for principals
type ACLCache struct {
	config          *ACLConfig
	cache           map[string]*ACLCacheEntry
	cacheMutex      sync.RWMutex
	client          *ACLClient
	singleflight    *singleflight.Group
	circuitBreaker  *ACLCircuitBreaker
	
	// Cleanup tracking
	lastCleanup     time.Time
	cleanupMutex    sync.Mutex
}

// ACLCacheEntry represents a cached permission entry
type ACLCacheEntry struct {
	Principal    string
	Permissions  *PermissionSet
	CachedAt     time.Time
	ExpiresAt    time.Time
	AccessCount  int64
	LastAccessed time.Time
}

// ACLClient handles communication with the ACL server
type ACLClient struct {
	endpoints       []string
	requestTimeout  time.Duration
	currentEndpoint int
	mutex          sync.RWMutex
}

// ACLCircuitBreaker implements circuit breaker pattern for ACL requests
type ACLCircuitBreaker struct {
	config       *ACLCircuitBreakerConfig
	state        ACLCircuitBreakerState
	failures     int
	successes    int
	lastFailure  time.Time
	mutex        sync.RWMutex
}

// ACLCircuitBreakerState represents circuit breaker states
type ACLCircuitBreakerState int

const (
	ACLCircuitBreakerClosed ACLCircuitBreakerState = iota
	ACLCircuitBreakerOpen
	ACLCircuitBreakerHalfOpen
)

// NewACLCache creates a new ACL cache
func NewACLCache(config *ACLConfig) (*ACLCache, error) {
	if config == nil {
		config = &ACLConfig{
			Enabled: false,
		}
	}
	
	cache := &ACLCache{
		config:         config,
		cache:          make(map[string]*ACLCacheEntry),
		singleflight:   &singleflight.Group{},
		lastCleanup:    time.Now(),
	}
	
	if config.Enabled {
		// Initialize ACL client
		client, err := NewACLClient(config)
		if err != nil {
			return nil, fmt.Errorf("failed to initialize ACL client: %w", err)
		}
		cache.client = client
		
		// Initialize circuit breaker
		if config.CircuitBreaker != nil {
			cache.circuitBreaker = NewACLCircuitBreaker(config.CircuitBreaker)
		}
	}
	
	return cache, nil
}

// GetPermissions retrieves permissions for a principal
func (c *ACLCache) GetPermissions(ctx context.Context, principal string) (*PermissionSet, error) {
	if !c.config.Enabled {
		// Return default permissions when ACL is disabled
		return &PermissionSet{}, nil
	}
	
	// Check cache first
	if permissions := c.getCachedPermissions(principal); permissions != nil {
		return permissions, nil
	}
	
	// Use singleflight to prevent duplicate requests
	if c.config.Cache != nil && c.config.Cache.EnableDeduplication {
		result, err, _ := c.singleflight.Do(principal, func() (interface{}, error) {
			return c.fetchPermissionsFromServer(ctx, principal)
		})
		
		if err != nil {
			return nil, err
		}
		
		permissions := result.(*PermissionSet)
		c.cachePermissions(principal, permissions)
		return permissions, nil
	}
	
	// Fetch directly without deduplication
	permissions, err := c.fetchPermissionsFromServer(ctx, principal)
	if err != nil {
		return nil, err
	}
	
	c.cachePermissions(principal, permissions)
	return permissions, nil
}

// getCachedPermissions retrieves permissions from cache
func (c *ACLCache) getCachedPermissions(principal string) *PermissionSet {
	c.cacheMutex.RLock()
	defer c.cacheMutex.RUnlock()
	
	entry, exists := c.cache[principal]
	if !exists {
		return nil
	}
	
	// Check if entry is expired
	if time.Now().After(entry.ExpiresAt) {
		return nil
	}
	
	// Update access statistics
	entry.AccessCount++
	entry.LastAccessed = time.Now()
	
	return entry.Permissions
}

// cachePermissions stores permissions in cache
func (c *ACLCache) cachePermissions(principal string, permissions *PermissionSet) {
	if c.config.Cache == nil {
		return
	}
	
	c.cacheMutex.Lock()
	defer c.cacheMutex.Unlock()
	
	// Check cache size and evict if necessary
	if len(c.cache) >= c.config.Cache.Size {
		c.evictOldEntries()
	}
	
	now := time.Now()
	expiresAt := now.Add(c.config.Cache.TTL)
	
	c.cache[principal] = &ACLCacheEntry{
		Principal:    principal,
		Permissions:  permissions,
		CachedAt:     now,
		ExpiresAt:    expiresAt,
		AccessCount:  1,
		LastAccessed: now,
	}
}

// evictOldEntries removes old cache entries using LRU strategy
func (c *ACLCache) evictOldEntries() {
	// Find the oldest entry by last accessed time
	var oldestKey string
	var oldestTime time.Time
	
	for key, entry := range c.cache {
		if oldestKey == "" || entry.LastAccessed.Before(oldestTime) {
			oldestKey = key
			oldestTime = entry.LastAccessed
		}
	}
	
	if oldestKey != "" {
		delete(c.cache, oldestKey)
	}
}

// fetchPermissionsFromServer retrieves permissions from the ACL server
func (c *ACLCache) fetchPermissionsFromServer(ctx context.Context, principal string) (*PermissionSet, error) {
	if c.circuitBreaker != nil && !c.circuitBreaker.CanExecute() {
		return nil, fmt.Errorf("ACL service unavailable (circuit breaker open)")
	}
	
	permissions, err := c.client.GetPermissions(ctx, principal)
	
	if c.circuitBreaker != nil {
		if err != nil {
			c.circuitBreaker.RecordFailure()
		} else {
			c.circuitBreaker.RecordSuccess()
		}
	}
	
	if err != nil {
		return nil, fmt.Errorf("failed to fetch permissions from server: %w", err)
	}
	
	return permissions, nil
}

// Cleanup removes expired cache entries
func (c *ACLCache) Cleanup() {
	c.cleanupMutex.Lock()
	defer c.cleanupMutex.Unlock()
	
	// Only run cleanup if enough time has passed
	if c.config.Cache == nil || time.Since(c.lastCleanup) < c.config.Cache.CleanupInterval {
		return
	}
	
	c.cacheMutex.Lock()
	defer c.cacheMutex.Unlock()
	
	now := time.Now()
	for principal, entry := range c.cache {
		if now.After(entry.ExpiresAt) {
			delete(c.cache, principal)
		}
	}
	
	c.lastCleanup = now
}

// GetStats returns cache statistics
func (c *ACLCache) GetStats() *ACLCacheStats {
	c.cacheMutex.RLock()
	defer c.cacheMutex.RUnlock()
	
	stats := &ACLCacheStats{
		Size:         len(c.cache),
		MaxSize:      c.config.Cache.Size,
		HitCount:     0,
		MissCount:    0,
		EvictionCount: 0,
	}
	
	now := time.Now()
	for _, entry := range c.cache {
		stats.HitCount += entry.AccessCount
		if now.After(entry.ExpiresAt) {
			stats.ExpiredCount++
		}
	}
	
	return stats
}

// Close shuts down the ACL cache
func (c *ACLCache) Close() {
	// Clear cache
	c.cacheMutex.Lock()
	c.cache = make(map[string]*ACLCacheEntry)
	c.cacheMutex.Unlock()
	
	// Close ACL client if it exists
	if c.client != nil {
		c.client.Close()
	}
}

// ACLCacheStats represents cache statistics
type ACLCacheStats struct {
	Size          int   `json:"size"`
	MaxSize       int   `json:"max_size"`
	HitCount      int64 `json:"hit_count"`
	MissCount     int64 `json:"miss_count"`
	EvictionCount int64 `json:"eviction_count"`
	ExpiredCount  int64 `json:"expired_count"`
}

// NewACLClient creates a new ACL client
func NewACLClient(config *ACLConfig) (*ACLClient, error) {
	if len(config.ControllerEndpoints) == 0 {
		return nil, fmt.Errorf("no controller endpoints provided")
	}
	
	return &ACLClient{
		endpoints:      config.ControllerEndpoints,
		requestTimeout: config.RequestTimeout,
	}, nil
}

// GetPermissions retrieves permissions for a principal from the server
func (c *ACLClient) GetPermissions(ctx context.Context, principal string) (*PermissionSet, error) {
	// TODO: Implement actual gRPC call to controller
	// This would involve:
	// 1. Selecting an available endpoint
	// 2. Creating a gRPC client
	// 3. Making a GetPermissions RPC call
	// 4. Parsing the response
	
	// For now, return default permissions based on principal
	permissions := &PermissionSet{}
	
	// Simple permission assignment for demo purposes
	if principal == "admin" {
		permissions.ReadTopics = []string{"*"}
		permissions.WriteTopics = []string{"*"}
		permissions.AdminOperations = []string{"*"}
	} else if principal != "anonymous" {
		permissions.ReadTopics = []string{fmt.Sprintf("%s.*", principal)}
		permissions.WriteTopics = []string{fmt.Sprintf("%s.*", principal)}
	}
	
	return permissions, nil
}

// Close shuts down the ACL client
func (c *ACLClient) Close() {
	// TODO: Close gRPC connections
}

// NewACLCircuitBreaker creates a new circuit breaker
func NewACLCircuitBreaker(config *ACLCircuitBreakerConfig) *ACLCircuitBreaker {
	return &ACLCircuitBreaker{
		config: config,
		state:  ACLCircuitBreakerClosed,
	}
}

// CanExecute checks if the circuit breaker allows execution
func (cb *ACLCircuitBreaker) CanExecute() bool {
	cb.mutex.RLock()
	defer cb.mutex.RUnlock()
	
	switch cb.state {
	case ACLCircuitBreakerClosed:
		return true
	case ACLCircuitBreakerOpen:
		// Check if timeout has passed
		if time.Since(cb.lastFailure) > cb.config.Timeout {
			cb.mutex.RUnlock()
			cb.mutex.Lock()
			cb.state = ACLCircuitBreakerHalfOpen
			cb.mutex.Unlock()
			cb.mutex.RLock()
			return true
		}
		return false
	case ACLCircuitBreakerHalfOpen:
		return true
	default:
		return false
	}
}

// RecordSuccess records a successful operation
func (cb *ACLCircuitBreaker) RecordSuccess() {
	cb.mutex.Lock()
	defer cb.mutex.Unlock()
	
	cb.successes++
	
	if cb.state == ACLCircuitBreakerHalfOpen {
		if cb.successes >= cb.config.SuccessThreshold {
			cb.state = ACLCircuitBreakerClosed
			cb.failures = 0
			cb.successes = 0
		}
	}
}

// RecordFailure records a failed operation
func (cb *ACLCircuitBreaker) RecordFailure() {
	cb.mutex.Lock()
	defer cb.mutex.Unlock()
	
	cb.failures++
	cb.lastFailure = time.Now()
	
	if cb.state == ACLCircuitBreakerClosed {
		if cb.failures >= cb.config.FailureThreshold {
			cb.state = ACLCircuitBreakerOpen
		}
	} else if cb.state == ACLCircuitBreakerHalfOpen {
		cb.state = ACLCircuitBreakerOpen
		cb.successes = 0
	}
}

// GetState returns the current circuit breaker state
func (cb *ACLCircuitBreaker) GetState() ACLCircuitBreakerState {
	cb.mutex.RLock()
	defer cb.mutex.RUnlock()
	return cb.state
}

// GetStats returns circuit breaker statistics
func (cb *ACLCircuitBreaker) GetStats() *ACLCircuitBreakerStats {
	cb.mutex.RLock()
	defer cb.mutex.RUnlock()
	
	return &ACLCircuitBreakerStats{
		State:        cb.state,
		Failures:     cb.failures,
		Successes:    cb.successes,
		LastFailure:  cb.lastFailure,
	}
}

// ACLCircuitBreakerStats represents circuit breaker statistics
type ACLCircuitBreakerStats struct {
	State       ACLCircuitBreakerState `json:"state"`
	Failures    int                    `json:"failures"`
	Successes   int                    `json:"successes"`
	LastFailure time.Time              `json:"last_failure"`
}