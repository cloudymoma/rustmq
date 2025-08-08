package rustmq

import (
	"crypto/x509"
	"fmt"
	"sync"
	"time"
)

// CertificateManager manages certificate validation and lifecycle
type CertificateManager struct {
	config       *CertificateValidationConfig
	validators   []CertificateValidator
	crlCache     *CRLCache
	ocspClient   *OCSPClient
	mu           sync.RWMutex
	
	// Certificate cache for performance
	certCache    map[string]*CertificateCacheEntry
	cacheMutex   sync.RWMutex
}

// CertificateCacheEntry represents a cached certificate entry
type CertificateCacheEntry struct {
	Certificate   *x509.Certificate
	ValidationResult *ValidationResult
	CachedAt      time.Time
	ExpiresAt     time.Time
}

// ValidationResult represents certificate validation result
type ValidationResult struct {
	Valid          bool
	Errors         []string
	Warnings       []string
	ValidatedAt    time.Time
	
	// Validation details
	ChainValid     bool
	NotExpired     bool
	NotRevoked     bool
	CustomChecks   map[string]bool
}

// CertificateValidator interface for certificate validation
type CertificateValidator interface {
	Validate(cert *x509.Certificate, chain []*x509.Certificate) *ValidationResult
	Name() string
}

// NewCertificateManager creates a new certificate manager
func NewCertificateManager(config *CertificateValidationConfig) (*CertificateManager, error) {
	if config == nil {
		config = &CertificateValidationConfig{
			Enabled: true,
		}
	}
	
	manager := &CertificateManager{
		config:    config,
		certCache: make(map[string]*CertificateCacheEntry),
	}
	
	// Initialize validators
	if err := manager.initializeValidators(); err != nil {
		return nil, fmt.Errorf("failed to initialize validators: %w", err)
	}
	
	// Initialize CRL cache if enabled
	if config.RevocationCheck != nil && config.RevocationCheck.EnableCRL {
		manager.crlCache = NewCRLCache(config.RevocationCheck.CRLCacheTTL)
	}
	
	// Initialize OCSP client if enabled
	if config.RevocationCheck != nil && config.RevocationCheck.EnableOCSP {
		manager.ocspClient = NewOCSPClient(config.RevocationCheck.OCSPTimeout)
	}
	
	return manager, nil
}

// initializeValidators sets up certificate validators
func (cm *CertificateManager) initializeValidators() error {
	cm.validators = []CertificateValidator{
		&BasicCertificateValidator{},
		&ExpirationValidator{},
	}
	
	// Add custom validators based on configuration
	if cm.config.Rules != nil {
		for _, rule := range cm.config.Rules {
			validator, err := cm.createCustomValidator(rule)
			if err != nil {
				return fmt.Errorf("failed to create custom validator for rule %s: %w", rule.Type, err)
			}
			if validator != nil {
				cm.validators = append(cm.validators, validator)
			}
		}
	}
	
	return nil
}

// ValidateCertificate validates a certificate
func (cm *CertificateManager) ValidateCertificate(cert *x509.Certificate) error {
	if !cm.config.Enabled {
		return nil
	}
	
	// Check cache first
	fingerprint := cm.getCertificateFingerprint(cert)
	if cached := cm.getCachedValidation(fingerprint); cached != nil {
		if !cached.Valid {
			return fmt.Errorf("certificate validation failed: %v", cached.Errors)
		}
		return nil
	}
	
	// Perform validation
	result := cm.performValidation(cert)
	
	// Cache the result
	cm.cacheValidationResult(fingerprint, cert, result)
	
	if !result.Valid {
		return fmt.Errorf("certificate validation failed: %v", result.Errors)
	}
	
	return nil
}

// performValidation performs the actual certificate validation
func (cm *CertificateManager) performValidation(cert *x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:        true,
		Errors:       []string{},
		Warnings:     []string{},
		ValidatedAt:  time.Now(),
		CustomChecks: make(map[string]bool),
	}
	
	// Run all validators
	for _, validator := range cm.validators {
		validatorResult := validator.Validate(cert, nil)
		
		// Merge results
		if !validatorResult.Valid {
			result.Valid = false
		}
		
		result.Errors = append(result.Errors, validatorResult.Errors...)
		result.Warnings = append(result.Warnings, validatorResult.Warnings...)
		
		// Merge custom checks
		for key, value := range validatorResult.CustomChecks {
			result.CustomChecks[key] = value
		}
		
		// Update specific validation flags
		if validator.Name() == "basic" {
			result.ChainValid = validatorResult.Valid
		} else if validator.Name() == "expiration" {
			result.NotExpired = validatorResult.Valid
		}
	}
	
	// Check revocation if enabled
	if cm.config.RevocationCheck != nil {
		revokedResult := cm.checkRevocation(cert)
		result.NotRevoked = revokedResult.Valid
		if !revokedResult.Valid {
			result.Valid = false
			result.Errors = append(result.Errors, revokedResult.Errors...)
		}
	}
	
	return result
}

// checkRevocation checks certificate revocation status
func (cm *CertificateManager) checkRevocation(cert *x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:    true,
		Errors:   []string{},
		Warnings: []string{},
	}
	
	// Check CRL if enabled
	if cm.crlCache != nil {
		if revoked, err := cm.crlCache.IsRevoked(cert); err != nil {
			result.Warnings = append(result.Warnings, fmt.Sprintf("CRL check failed: %v", err))
		} else if revoked {
			result.Valid = false
			result.Errors = append(result.Errors, "certificate is revoked (CRL)")
		}
	}
	
	// Check OCSP if enabled
	if cm.ocspClient != nil {
		if revoked, err := cm.ocspClient.IsRevoked(cert); err != nil {
			result.Warnings = append(result.Warnings, fmt.Sprintf("OCSP check failed: %v", err))
		} else if revoked {
			result.Valid = false
			result.Errors = append(result.Errors, "certificate is revoked (OCSP)")
		}
	}
	
	return result
}

// getCertificateFingerprint generates a fingerprint for the certificate
func (cm *CertificateManager) getCertificateFingerprint(cert *x509.Certificate) string {
	return fmt.Sprintf("%x", cert.Raw)
}

// getCachedValidation retrieves cached validation result
func (cm *CertificateManager) getCachedValidation(fingerprint string) *ValidationResult {
	cm.cacheMutex.RLock()
	defer cm.cacheMutex.RUnlock()
	
	entry, exists := cm.certCache[fingerprint]
	if !exists {
		return nil
	}
	
	// Check if cache entry is still valid
	if time.Now().After(entry.ExpiresAt) {
		return nil
	}
	
	return entry.ValidationResult
}

// cacheValidationResult caches a validation result
func (cm *CertificateManager) cacheValidationResult(fingerprint string, cert *x509.Certificate, result *ValidationResult) {
	cm.cacheMutex.Lock()
	defer cm.cacheMutex.Unlock()
	
	// Cache for 1 hour or until certificate expires, whichever is shorter
	cacheExpiry := time.Now().Add(1 * time.Hour)
	if cert.NotAfter.Before(cacheExpiry) {
		cacheExpiry = cert.NotAfter
	}
	
	cm.certCache[fingerprint] = &CertificateCacheEntry{
		Certificate:      cert,
		ValidationResult: result,
		CachedAt:         time.Now(),
		ExpiresAt:        cacheExpiry,
	}
}

// createCustomValidator creates a custom validator based on rule
func (cm *CertificateManager) createCustomValidator(rule CertificateValidationRule) (CertificateValidator, error) {
	switch rule.Type {
	case "subject_pattern":
		if pattern, ok := rule.Parameters["pattern"].(string); ok {
			return &SubjectPatternValidator{Pattern: pattern}, nil
		}
		return nil, fmt.Errorf("subject_pattern validator requires 'pattern' parameter")
	case "key_usage":
		if usage, ok := rule.Parameters["usage"].(string); ok {
			return &KeyUsageValidator{RequiredUsage: usage}, nil
		}
		return nil, fmt.Errorf("key_usage validator requires 'usage' parameter")
	default:
		return nil, fmt.Errorf("unknown validator type: %s", rule.Type)
	}
}

// Cleanup removes expired cache entries
func (cm *CertificateManager) Cleanup() {
	cm.cacheMutex.Lock()
	defer cm.cacheMutex.Unlock()
	
	now := time.Now()
	for fingerprint, entry := range cm.certCache {
		if now.After(entry.ExpiresAt) {
			delete(cm.certCache, fingerprint)
		}
	}
}

// BasicCertificateValidator performs basic certificate validation
type BasicCertificateValidator struct{}

func (v *BasicCertificateValidator) Validate(cert *x509.Certificate, chain []*x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:        true,
		Errors:       []string{},
		Warnings:     []string{},
		CustomChecks: make(map[string]bool),
	}
	
	// Basic certificate checks
	if cert == nil {
		result.Valid = false
		result.Errors = append(result.Errors, "certificate is nil")
		return result
	}
	
	// Check certificate format
	if len(cert.Raw) == 0 {
		result.Valid = false
		result.Errors = append(result.Errors, "certificate has no raw data")
	}
	
	// Check subject
	if cert.Subject.String() == "" {
		result.Warnings = append(result.Warnings, "certificate has empty subject")
	}
	
	result.CustomChecks["has_subject"] = cert.Subject.String() != ""
	result.CustomChecks["has_raw_data"] = len(cert.Raw) > 0
	
	return result
}

func (v *BasicCertificateValidator) Name() string {
	return "basic"
}

// ExpirationValidator validates certificate expiration
type ExpirationValidator struct{}

func (v *ExpirationValidator) Validate(cert *x509.Certificate, chain []*x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:        true,
		Errors:       []string{},
		Warnings:     []string{},
		CustomChecks: make(map[string]bool),
	}
	
	now := time.Now()
	
	// Check if certificate is not yet valid
	if now.Before(cert.NotBefore) {
		result.Valid = false
		result.Errors = append(result.Errors, fmt.Sprintf("certificate not valid until %v", cert.NotBefore))
	}
	
	// Check if certificate is expired
	if now.After(cert.NotAfter) {
		result.Valid = false
		result.Errors = append(result.Errors, fmt.Sprintf("certificate expired at %v", cert.NotAfter))
	}
	
	// Check if certificate expires soon (within 30 days)
	expiryThreshold := now.Add(30 * 24 * time.Hour)
	if cert.NotAfter.Before(expiryThreshold) {
		result.Warnings = append(result.Warnings, fmt.Sprintf("certificate expires soon: %v", cert.NotAfter))
	}
	
	result.CustomChecks["not_expired"] = now.Before(cert.NotAfter)
	result.CustomChecks["not_before_valid"] = now.After(cert.NotBefore)
	result.CustomChecks["expires_soon"] = cert.NotAfter.Before(expiryThreshold)
	
	return result
}

func (v *ExpirationValidator) Name() string {
	return "expiration"
}

// SubjectPatternValidator validates certificate subject against pattern
type SubjectPatternValidator struct {
	Pattern string
}

func (v *SubjectPatternValidator) Validate(cert *x509.Certificate, chain []*x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:        true,
		Errors:       []string{},
		Warnings:     []string{},
		CustomChecks: make(map[string]bool),
	}
	
	subject := cert.Subject.String()
	matches := matchesPattern(subject, v.Pattern)
	
	if !matches {
		result.Valid = false
		result.Errors = append(result.Errors, fmt.Sprintf("certificate subject '%s' does not match pattern '%s'", subject, v.Pattern))
	}
	
	result.CustomChecks["subject_pattern_match"] = matches
	
	return result
}

func (v *SubjectPatternValidator) Name() string {
	return "subject_pattern"
}

// KeyUsageValidator validates certificate key usage
type KeyUsageValidator struct {
	RequiredUsage string
}

func (v *KeyUsageValidator) Validate(cert *x509.Certificate, chain []*x509.Certificate) *ValidationResult {
	result := &ValidationResult{
		Valid:        true,
		Errors:       []string{},
		Warnings:     []string{},
		CustomChecks: make(map[string]bool),
	}
	
	// Check key usage based on required usage
	switch v.RequiredUsage {
	case "digital_signature":
		if cert.KeyUsage&x509.KeyUsageDigitalSignature == 0 {
			result.Valid = false
			result.Errors = append(result.Errors, "certificate does not allow digital signature")
		}
		result.CustomChecks["digital_signature"] = cert.KeyUsage&x509.KeyUsageDigitalSignature != 0
		
	case "key_encipherment":
		if cert.KeyUsage&x509.KeyUsageKeyEncipherment == 0 {
			result.Valid = false
			result.Errors = append(result.Errors, "certificate does not allow key encipherment")
		}
		result.CustomChecks["key_encipherment"] = cert.KeyUsage&x509.KeyUsageKeyEncipherment != 0
		
	case "client_auth":
		hasClientAuth := false
		for _, usage := range cert.ExtKeyUsage {
			if usage == x509.ExtKeyUsageClientAuth {
				hasClientAuth = true
				break
			}
		}
		if !hasClientAuth {
			result.Valid = false
			result.Errors = append(result.Errors, "certificate does not allow client authentication")
		}
		result.CustomChecks["client_auth"] = hasClientAuth
		
	default:
		result.Warnings = append(result.Warnings, fmt.Sprintf("unknown key usage requirement: %s", v.RequiredUsage))
	}
	
	return result
}

func (v *KeyUsageValidator) Name() string {
	return "key_usage"
}

// CRLCache manages Certificate Revocation List cache
type CRLCache struct {
	cacheTTL time.Duration
	cache    map[string]*CRLEntry
	mutex    sync.RWMutex
}

// CRLEntry represents a CRL cache entry
type CRLEntry struct {
	CRL       *x509.RevocationList
	CachedAt  time.Time
	ExpiresAt time.Time
}

// NewCRLCache creates a new CRL cache
func NewCRLCache(ttl time.Duration) *CRLCache {
	return &CRLCache{
		cacheTTL: ttl,
		cache:    make(map[string]*CRLEntry),
	}
}

// IsRevoked checks if a certificate is revoked using CRL
func (c *CRLCache) IsRevoked(cert *x509.Certificate) (bool, error) {
	// TODO: Implement CRL checking
	// This would involve:
	// 1. Finding the CRL distribution points from the certificate
	// 2. Downloading the CRL if not cached
	// 3. Checking if the certificate serial number is in the revocation list
	
	return false, nil
}

// OCSPClient manages OCSP (Online Certificate Status Protocol) checking
type OCSPClient struct {
	timeout time.Duration
}

// NewOCSPClient creates a new OCSP client
func NewOCSPClient(timeout time.Duration) *OCSPClient {
	return &OCSPClient{
		timeout: timeout,
	}
}

// IsRevoked checks if a certificate is revoked using OCSP
func (o *OCSPClient) IsRevoked(cert *x509.Certificate) (bool, error) {
	// TODO: Implement OCSP checking
	// This would involve:
	// 1. Finding the OCSP responder URL from the certificate
	// 2. Creating an OCSP request
	// 3. Sending the request to the OCSP responder
	// 4. Parsing the response to determine revocation status
	
	return false, nil
}