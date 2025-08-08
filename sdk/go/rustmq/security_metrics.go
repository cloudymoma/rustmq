package rustmq

import (
	"context"
	"sync"
	"sync/atomic"
	"time"
)

// SecurityMetrics collects and manages security-related metrics
type SecurityMetrics struct {
	config              *SecurityMetricsConfig
	
	// Authentication metrics
	authAttempts        map[string]*int64  // method -> count
	authSuccesses       map[string]*int64  // method -> count
	authFailures        map[string]*int64  // method -> count
	authDurations       map[string]*authDurationStats
	authMutex           sync.RWMutex
	
	// Authorization metrics
	aclRequests         *int64
	aclCacheHits        *int64
	aclCacheMisses      *int64
	aclErrors           *int64
	aclDurations        *durationStats
	
	// Certificate metrics
	certValidations     *int64
	certValidationErrors *int64
	certExpiringCount   *int64
	certDurations       *durationStats
	
	// TLS metrics
	tlsConnections      *int64
	tlsHandshakeErrors  *int64
	tlsHandshakeDurations *durationStats
	
	// Security context metrics
	activeContexts      *int64
	contextCreations    *int64
	contextExpirations  *int64
	
	// General metrics
	startTime           time.Time
	lastCollection      time.Time
	collectionCount     *int64
	
	// Background collection
	ctx                 context.Context
	cancel              context.CancelFunc
	workers             sync.WaitGroup
}

// authDurationStats tracks authentication duration statistics
type authDurationStats struct {
	totalDuration *int64 // nanoseconds
	count        *int64
	minDuration  *int64 // nanoseconds
	maxDuration  *int64 // nanoseconds
	mutex        sync.RWMutex
}

// durationStats tracks generic duration statistics
type durationStats struct {
	totalDuration *int64 // nanoseconds
	count        *int64
	minDuration  *int64 // nanoseconds
	maxDuration  *int64 // nanoseconds
}

// NewSecurityMetrics creates a new security metrics collector
func NewSecurityMetrics(config *SecurityMetricsConfig) *SecurityMetrics {
	if config == nil {
		config = &SecurityMetricsConfig{
			Enabled:            false,
			CollectionInterval: 1 * time.Minute,
			Detailed:           false,
		}
	}
	
	ctx, cancel := context.WithCancel(context.Background())
	
	metrics := &SecurityMetrics{
		config:              config,
		authAttempts:        make(map[string]*int64),
		authSuccesses:       make(map[string]*int64),
		authFailures:        make(map[string]*int64),
		authDurations:       make(map[string]*authDurationStats),
		aclRequests:         new(int64),
		aclCacheHits:        new(int64),
		aclCacheMisses:      new(int64),
		aclErrors:           new(int64),
		aclDurations:        &durationStats{totalDuration: new(int64), count: new(int64), minDuration: new(int64), maxDuration: new(int64)},
		certValidations:     new(int64),
		certValidationErrors: new(int64),
		certExpiringCount:   new(int64),
		certDurations:       &durationStats{totalDuration: new(int64), count: new(int64), minDuration: new(int64), maxDuration: new(int64)},
		tlsConnections:      new(int64),
		tlsHandshakeErrors:  new(int64),
		tlsHandshakeDurations: &durationStats{totalDuration: new(int64), count: new(int64), minDuration: new(int64), maxDuration: new(int64)},
		activeContexts:      new(int64),
		contextCreations:    new(int64),
		contextExpirations:  new(int64),
		startTime:           time.Now(),
		lastCollection:      time.Now(),
		collectionCount:     new(int64),
		ctx:                 ctx,
		cancel:              cancel,
	}
	
	// Initialize min durations to max value
	atomic.StoreInt64(metrics.aclDurations.minDuration, int64(^uint64(0)>>1))
	atomic.StoreInt64(metrics.certDurations.minDuration, int64(^uint64(0)>>1))
	atomic.StoreInt64(metrics.tlsHandshakeDurations.minDuration, int64(^uint64(0)>>1))
	
	if config.Enabled {
		// Start background collection worker
		metrics.workers.Add(1)
		go metrics.collectionWorker()
	}
	
	return metrics
}

// RecordAuthentication records an authentication attempt
func (sm *SecurityMetrics) RecordAuthentication(method string, success bool) {
	if !sm.config.Enabled {
		return
	}
	
	sm.ensureAuthMethodStats(method)
	
	atomic.AddInt64(sm.authAttempts[method], 1)
	
	if success {
		atomic.AddInt64(sm.authSuccesses[method], 1)
	} else {
		atomic.AddInt64(sm.authFailures[method], 1)
	}
}

// RecordAuthenticationDuration records authentication duration
func (sm *SecurityMetrics) RecordAuthenticationDuration(method string, duration time.Duration) {
	if !sm.config.Enabled || !sm.config.Detailed {
		return
	}
	
	sm.ensureAuthMethodStats(method)
	
	durationNs := duration.Nanoseconds()
	stats := sm.authDurations[method]
	
	atomic.AddInt64(stats.totalDuration, durationNs)
	atomic.AddInt64(stats.count, 1)
	
	// Update min/max
	stats.mutex.Lock()
	currentMin := atomic.LoadInt64(stats.minDuration)
	if durationNs < currentMin {
		atomic.StoreInt64(stats.minDuration, durationNs)
	}
	
	currentMax := atomic.LoadInt64(stats.maxDuration)
	if durationNs > currentMax {
		atomic.StoreInt64(stats.maxDuration, durationNs)
	}
	stats.mutex.Unlock()
}

// RecordACLRequest records an ACL request
func (sm *SecurityMetrics) RecordACLRequest(hit bool, duration time.Duration) {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.aclRequests, 1)
	
	if hit {
		atomic.AddInt64(sm.aclCacheHits, 1)
	} else {
		atomic.AddInt64(sm.aclCacheMisses, 1)
	}
	
	if sm.config.Detailed {
		sm.recordDuration(sm.aclDurations, duration)
	}
}

// RecordACLError records an ACL error
func (sm *SecurityMetrics) RecordACLError() {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.aclErrors, 1)
}

// RecordCertificateValidation records a certificate validation
func (sm *SecurityMetrics) RecordCertificateValidation(success bool, duration time.Duration) {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.certValidations, 1)
	
	if !success {
		atomic.AddInt64(sm.certValidationErrors, 1)
	}
	
	if sm.config.Detailed {
		sm.recordDuration(sm.certDurations, duration)
	}
}

// RecordCertificateExpiring records a certificate nearing expiration
func (sm *SecurityMetrics) RecordCertificateExpiring() {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.certExpiringCount, 1)
}

// RecordTLSConnection records a TLS connection
func (sm *SecurityMetrics) RecordTLSConnection(success bool, handshakeDuration time.Duration) {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.tlsConnections, 1)
	
	if !success {
		atomic.AddInt64(sm.tlsHandshakeErrors, 1)
	}
	
	if sm.config.Detailed {
		sm.recordDuration(sm.tlsHandshakeDurations, handshakeDuration)
	}
}

// RecordSecurityContextCreation records security context creation
func (sm *SecurityMetrics) RecordSecurityContextCreation() {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.contextCreations, 1)
	atomic.AddInt64(sm.activeContexts, 1)
}

// RecordSecurityContextExpiration records security context expiration
func (sm *SecurityMetrics) RecordSecurityContextExpiration() {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.contextExpirations, 1)
	atomic.AddInt64(sm.activeContexts, -1)
}

// ensureAuthMethodStats ensures authentication method statistics exist
func (sm *SecurityMetrics) ensureAuthMethodStats(method string) {
	sm.authMutex.RLock()
	if _, exists := sm.authAttempts[method]; exists {
		sm.authMutex.RUnlock()
		return
	}
	sm.authMutex.RUnlock()
	
	sm.authMutex.Lock()
	defer sm.authMutex.Unlock()
	
	// Double-check after acquiring write lock
	if _, exists := sm.authAttempts[method]; exists {
		return
	}
	
	sm.authAttempts[method] = new(int64)
	sm.authSuccesses[method] = new(int64)
	sm.authFailures[method] = new(int64)
	sm.authDurations[method] = &authDurationStats{
		totalDuration: new(int64),
		count:        new(int64),
		minDuration:  new(int64),
		maxDuration:  new(int64),
	}
	
	// Initialize min duration to max value
	atomic.StoreInt64(sm.authDurations[method].minDuration, int64(^uint64(0)>>1))
}

// recordDuration records a duration in the given duration stats
func (sm *SecurityMetrics) recordDuration(stats *durationStats, duration time.Duration) {
	durationNs := duration.Nanoseconds()
	
	atomic.AddInt64(stats.totalDuration, durationNs)
	atomic.AddInt64(stats.count, 1)
	
	// Update min/max atomically
	for {
		currentMin := atomic.LoadInt64(stats.minDuration)
		if durationNs >= currentMin || atomic.CompareAndSwapInt64(stats.minDuration, currentMin, durationNs) {
			break
		}
	}
	
	for {
		currentMax := atomic.LoadInt64(stats.maxDuration)
		if durationNs <= currentMax || atomic.CompareAndSwapInt64(stats.maxDuration, currentMax, durationNs) {
			break
		}
	}
}

// GetMetrics returns current security metrics
func (sm *SecurityMetrics) GetMetrics() *SecurityMetricsSnapshot {
	snapshot := &SecurityMetricsSnapshot{
		Timestamp:       time.Now(),
		Uptime:          time.Since(sm.startTime),
		CollectionCount: atomic.LoadInt64(sm.collectionCount),
		
		// Authentication metrics
		AuthMetrics: make(map[string]*AuthMethodMetrics),
		
		// ACL metrics
		ACLMetrics: &ACLMetrics{
			Requests:   atomic.LoadInt64(sm.aclRequests),
			CacheHits:  atomic.LoadInt64(sm.aclCacheHits),
			CacheMisses: atomic.LoadInt64(sm.aclCacheMisses),
			Errors:     atomic.LoadInt64(sm.aclErrors),
		},
		
		// Certificate metrics
		CertificateMetrics: &CertificateMetrics{
			Validations: atomic.LoadInt64(sm.certValidations),
			Errors:      atomic.LoadInt64(sm.certValidationErrors),
			Expiring:    atomic.LoadInt64(sm.certExpiringCount),
		},
		
		// TLS metrics
		TLSMetrics: &TLSMetrics{
			Connections:      atomic.LoadInt64(sm.tlsConnections),
			HandshakeErrors:  atomic.LoadInt64(sm.tlsHandshakeErrors),
		},
		
		// Context metrics
		ContextMetrics: &ContextMetrics{
			Active:      atomic.LoadInt64(sm.activeContexts),
			Creations:   atomic.LoadInt64(sm.contextCreations),
			Expirations: atomic.LoadInt64(sm.contextExpirations),
		},
	}
	
	// Copy authentication metrics
	sm.authMutex.RLock()
	for method, attempts := range sm.authAttempts {
		authMetrics := &AuthMethodMetrics{
			Attempts:  atomic.LoadInt64(attempts),
			Successes: atomic.LoadInt64(sm.authSuccesses[method]),
			Failures:  atomic.LoadInt64(sm.authFailures[method]),
		}
		
		if sm.config.Detailed {
			authStats := sm.authDurations[method]
			count := atomic.LoadInt64(authStats.count)
			if count > 0 {
				authMetrics.Duration = &DurationMetrics{
					Count:   count,
					Total:   time.Duration(atomic.LoadInt64(authStats.totalDuration)),
					Average: time.Duration(atomic.LoadInt64(authStats.totalDuration) / count),
					Min:     time.Duration(atomic.LoadInt64(authStats.minDuration)),
					Max:     time.Duration(atomic.LoadInt64(authStats.maxDuration)),
				}
			}
		}
		
		snapshot.AuthMetrics[method] = authMetrics
	}
	sm.authMutex.RUnlock()
	
	// Add duration metrics if detailed mode is enabled
	if sm.config.Detailed {
		snapshot.ACLMetrics.Duration = sm.getDurationMetrics(sm.aclDurations)
		snapshot.CertificateMetrics.Duration = sm.getDurationMetrics(sm.certDurations)
		snapshot.TLSMetrics.Duration = sm.getDurationMetrics(sm.tlsHandshakeDurations)
	}
	
	return snapshot
}

// getDurationMetrics creates duration metrics from duration stats
func (sm *SecurityMetrics) getDurationMetrics(stats *durationStats) *DurationMetrics {
	count := atomic.LoadInt64(stats.count)
	if count == 0 {
		return nil
	}
	
	totalNs := atomic.LoadInt64(stats.totalDuration)
	minNs := atomic.LoadInt64(stats.minDuration)
	maxNs := atomic.LoadInt64(stats.maxDuration)
	
	return &DurationMetrics{
		Count:   count,
		Total:   time.Duration(totalNs),
		Average: time.Duration(totalNs / count),
		Min:     time.Duration(minNs),
		Max:     time.Duration(maxNs),
	}
}

// Collect triggers metrics collection (called by background worker)
func (sm *SecurityMetrics) Collect() {
	if !sm.config.Enabled {
		return
	}
	
	atomic.AddInt64(sm.collectionCount, 1)
	sm.lastCollection = time.Now()
	
	// Export metrics if configured
	if sm.config.Export != nil && sm.config.Export.Endpoint != "" {
		snapshot := sm.GetMetrics()
		sm.exportMetrics(snapshot)
	}
}

// exportMetrics exports metrics to the configured endpoint
func (sm *SecurityMetrics) exportMetrics(snapshot *SecurityMetricsSnapshot) {
	// TODO: Implement metrics export
	// This would send metrics to endpoints like:
	// - Prometheus endpoint
	// - JSON over HTTP
	// - StatsD
	// - Custom metrics systems
}

// collectionWorker runs periodic metrics collection
func (sm *SecurityMetrics) collectionWorker() {
	defer sm.workers.Done()
	
	ticker := time.NewTicker(sm.config.CollectionInterval)
	defer ticker.Stop()
	
	for {
		select {
		case <-sm.ctx.Done():
			return
		case <-ticker.C:
			sm.Collect()
		}
	}
}

// Close shuts down the security metrics collector
func (sm *SecurityMetrics) Close() {
	sm.cancel()
	sm.workers.Wait()
}

// SecurityMetricsSnapshot represents a point-in-time snapshot of security metrics
type SecurityMetricsSnapshot struct {
	Timestamp          time.Time                      `json:"timestamp"`
	Uptime             time.Duration                  `json:"uptime"`
	CollectionCount    int64                          `json:"collection_count"`
	AuthMetrics        map[string]*AuthMethodMetrics  `json:"auth_metrics"`
	ACLMetrics         *ACLMetrics                    `json:"acl_metrics"`
	CertificateMetrics *CertificateMetrics            `json:"certificate_metrics"`
	TLSMetrics         *TLSMetrics                    `json:"tls_metrics"`
	ContextMetrics     *ContextMetrics                `json:"context_metrics"`
}

// AuthMethodMetrics represents authentication method metrics
type AuthMethodMetrics struct {
	Attempts  int64            `json:"attempts"`
	Successes int64            `json:"successes"`
	Failures  int64            `json:"failures"`
	Duration  *DurationMetrics `json:"duration,omitempty"`
}

// ACLMetrics represents ACL-related metrics
type ACLMetrics struct {
	Requests    int64            `json:"requests"`
	CacheHits   int64            `json:"cache_hits"`
	CacheMisses int64            `json:"cache_misses"`
	Errors      int64            `json:"errors"`
	Duration    *DurationMetrics `json:"duration,omitempty"`
}

// CertificateMetrics represents certificate-related metrics
type CertificateMetrics struct {
	Validations int64            `json:"validations"`
	Errors      int64            `json:"errors"`
	Expiring    int64            `json:"expiring"`
	Duration    *DurationMetrics `json:"duration,omitempty"`
}

// TLSMetrics represents TLS-related metrics
type TLSMetrics struct {
	Connections      int64            `json:"connections"`
	HandshakeErrors  int64            `json:"handshake_errors"`
	Duration         *DurationMetrics `json:"duration,omitempty"`
}

// ContextMetrics represents security context metrics
type ContextMetrics struct {
	Active      int64 `json:"active"`
	Creations   int64 `json:"creations"`
	Expirations int64 `json:"expirations"`
}

// DurationMetrics represents duration statistics
type DurationMetrics struct {
	Count   int64         `json:"count"`
	Total   time.Duration `json:"total"`
	Average time.Duration `json:"average"`
	Min     time.Duration `json:"min"`
	Max     time.Duration `json:"max"`
}

// GetSuccessRate returns the authentication success rate for a method
func (am *AuthMethodMetrics) GetSuccessRate() float64 {
	if am.Attempts == 0 {
		return 0.0
	}
	return float64(am.Successes) / float64(am.Attempts)
}

// GetCacheHitRate returns the ACL cache hit rate
func (acl *ACLMetrics) GetCacheHitRate() float64 {
	total := acl.CacheHits + acl.CacheMisses
	if total == 0 {
		return 0.0
	}
	return float64(acl.CacheHits) / float64(total)
}

// GetErrorRate returns the certificate validation error rate
func (cm *CertificateMetrics) GetErrorRate() float64 {
	if cm.Validations == 0 {
		return 0.0
	}
	return float64(cm.Errors) / float64(cm.Validations)
}

// GetHandshakeErrorRate returns the TLS handshake error rate
func (tls *TLSMetrics) GetHandshakeErrorRate() float64 {
	if tls.Connections == 0 {
		return 0.0
	}
	return float64(tls.HandshakeErrors) / float64(tls.Connections)
}