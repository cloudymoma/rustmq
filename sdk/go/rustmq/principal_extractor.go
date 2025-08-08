package rustmq

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
	"sync"
)

// PrincipalExtractor extracts principals from certificate information
type PrincipalExtractor struct {
	config      *PrincipalExtractionConfig
	stringPool  map[string]string // String interning for memory efficiency
	poolMutex   sync.RWMutex
	rulesCache  []*compiledExtractionRule
	rulesMutex  sync.RWMutex
}

// compiledExtractionRule represents a compiled extraction rule
type compiledExtractionRule struct {
	rule     PrincipalExtractionRule
	regex    *regexp.Regexp
	priority int
}

// NewPrincipalExtractor creates a new principal extractor
func NewPrincipalExtractor(config *PrincipalExtractionConfig) *PrincipalExtractor {
	if config == nil {
		config = &PrincipalExtractionConfig{
			UseCommonName:     true,
			UseSubjectAltName: false,
			Normalize:         true,
		}
	}
	
	extractor := &PrincipalExtractor{
		config:     config,
		stringPool: make(map[string]string),
	}
	
	// Compile custom rules
	extractor.compileCustomRules()
	
	return extractor
}

// ExtractPrincipal extracts a principal from certificate information
func (pe *PrincipalExtractor) ExtractPrincipal(certInfo *CertificateInfo) (string, error) {
	if certInfo == nil {
		return "", fmt.Errorf("certificate information is nil")
	}
	
	// Try custom rules first (sorted by priority)
	if principal := pe.extractUsingCustomRules(certInfo); principal != "" {
		return pe.internString(pe.normalizePrincipal(principal)), nil
	}
	
	// Try common name extraction
	if pe.config.UseCommonName {
		if cn := certInfo.Attributes["CN"]; cn != "" {
			return pe.internString(pe.normalizePrincipal(cn)), nil
		}
	}
	
	// Try subject alternative name extraction
	if pe.config.UseSubjectAltName {
		if principal := pe.extractFromSubjectAltNames(certInfo); principal != "" {
			return pe.internString(pe.normalizePrincipal(principal)), nil
		}
	}
	
	// Fallback to full subject
	if certInfo.Subject != "" {
		return pe.internString(pe.normalizePrincipal(certInfo.Subject)), nil
	}
	
	return "", fmt.Errorf("unable to extract principal from certificate")
}

// extractUsingCustomRules tries to extract principal using custom rules
func (pe *PrincipalExtractor) extractUsingCustomRules(certInfo *CertificateInfo) string {
	pe.rulesMutex.RLock()
	defer pe.rulesMutex.RUnlock()
	
	for _, rule := range pe.rulesCache {
		if principal := pe.applyExtractionRule(rule, certInfo); principal != "" {
			return principal
		}
	}
	
	return ""
}

// applyExtractionRule applies a single extraction rule
func (pe *PrincipalExtractor) applyExtractionRule(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	switch rule.rule.Type {
	case "subject_attribute":
		return pe.extractFromSubjectAttribute(rule, certInfo)
	case "subject_pattern":
		return pe.extractFromSubjectPattern(rule, certInfo)
	case "issuer_pattern":
		return pe.extractFromIssuerPattern(rule, certInfo)
	case "san_email":
		return pe.extractFromSANEmail(rule, certInfo)
	case "san_dns":
		return pe.extractFromSANDNS(rule, certInfo)
	default:
		return ""
	}
}

// extractFromSubjectAttribute extracts principal from a specific subject attribute
func (pe *PrincipalExtractor) extractFromSubjectAttribute(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	attribute := rule.rule.Pattern
	if value, exists := certInfo.Attributes[attribute]; exists && value != "" {
		return value
	}
	return ""
}

// extractFromSubjectPattern extracts principal using regex pattern on subject
func (pe *PrincipalExtractor) extractFromSubjectPattern(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	if rule.regex == nil {
		return ""
	}
	
	matches := rule.regex.FindStringSubmatch(certInfo.Subject)
	if len(matches) > 1 {
		return matches[1] // Return first capturing group
	}
	
	return ""
}

// extractFromIssuerPattern extracts principal using regex pattern on issuer
func (pe *PrincipalExtractor) extractFromIssuerPattern(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	if rule.regex == nil {
		return ""
	}
	
	matches := rule.regex.FindStringSubmatch(certInfo.Issuer)
	if len(matches) > 1 {
		return matches[1]
	}
	
	return ""
}

// extractFromSANEmail extracts principal from Subject Alternative Name email
func (pe *PrincipalExtractor) extractFromSANEmail(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	for _, san := range certInfo.SubjectAltNames {
		if strings.Contains(san, "@") { // Simple email detection
			if rule.regex != nil {
				matches := rule.regex.FindStringSubmatch(san)
				if len(matches) > 1 {
					return matches[1]
				}
			} else {
				return san
			}
		}
	}
	return ""
}

// extractFromSANDNS extracts principal from Subject Alternative Name DNS entries
func (pe *PrincipalExtractor) extractFromSANDNS(rule *compiledExtractionRule, certInfo *CertificateInfo) string {
	for _, san := range certInfo.SubjectAltNames {
		if !strings.Contains(san, "@") { // Assume DNS name if no @
			if rule.regex != nil {
				matches := rule.regex.FindStringSubmatch(san)
				if len(matches) > 1 {
					return matches[1]
				}
			} else {
				return san
			}
		}
	}
	return ""
}

// extractFromSubjectAltNames extracts principal from subject alternative names
func (pe *PrincipalExtractor) extractFromSubjectAltNames(certInfo *CertificateInfo) string {
	if len(certInfo.SubjectAltNames) == 0 {
		return ""
	}
	
	// Prefer email addresses over DNS names
	for _, san := range certInfo.SubjectAltNames {
		if strings.Contains(san, "@") {
			return san
		}
	}
	
	// Fallback to first DNS name
	for _, san := range certInfo.SubjectAltNames {
		if !strings.Contains(san, "@") {
			return san
		}
	}
	
	return ""
}

// normalizePrincipal normalizes a principal based on configuration
func (pe *PrincipalExtractor) normalizePrincipal(principal string) string {
	if !pe.config.Normalize {
		return principal
	}
	
	// Trim whitespace and convert to lowercase
	normalized := strings.TrimSpace(strings.ToLower(principal))
	
	// Additional normalization rules could be added here
	// - Remove special characters
	// - Replace spaces with underscores
	// - Apply domain-specific transformations
	
	return normalized
}

// internString interns a string for memory efficiency
func (pe *PrincipalExtractor) internString(s string) string {
	pe.poolMutex.RLock()
	if interned, exists := pe.stringPool[s]; exists {
		pe.poolMutex.RUnlock()
		return interned
	}
	pe.poolMutex.RUnlock()
	
	pe.poolMutex.Lock()
	defer pe.poolMutex.Unlock()
	
	// Double-check after acquiring write lock
	if interned, exists := pe.stringPool[s]; exists {
		return interned
	}
	
	// Add to pool
	pe.stringPool[s] = s
	return s
}

// compileCustomRules compiles custom extraction rules
func (pe *PrincipalExtractor) compileCustomRules() {
	if pe.config.CustomRules == nil {
		return
	}
	
	var compiledRules []*compiledExtractionRule
	
	for _, rule := range pe.config.CustomRules {
		compiledRule := &compiledExtractionRule{
			rule:     rule,
			priority: rule.Priority,
		}
		
		// Compile regex pattern if provided
		if rule.Pattern != "" && (rule.Type == "subject_pattern" || rule.Type == "issuer_pattern" || rule.Type == "san_email" || rule.Type == "san_dns") {
			if regex, err := regexp.Compile(rule.Pattern); err == nil {
				compiledRule.regex = regex
			}
		}
		
		compiledRules = append(compiledRules, compiledRule)
	}
	
	// Sort rules by priority (higher priority first)
	sort.Slice(compiledRules, func(i, j int) bool {
		return compiledRules[i].priority > compiledRules[j].priority
	})
	
	pe.rulesMutex.Lock()
	pe.rulesCache = compiledRules
	pe.rulesMutex.Unlock()
}

// UpdateConfig updates the extractor configuration
func (pe *PrincipalExtractor) UpdateConfig(config *PrincipalExtractionConfig) {
	pe.config = config
	pe.compileCustomRules()
}

// GetStats returns principal extractor statistics
func (pe *PrincipalExtractor) GetStats() *PrincipalExtractorStats {
	pe.poolMutex.RLock()
	defer pe.poolMutex.RUnlock()
	
	return &PrincipalExtractorStats{
		StringPoolSize:    len(pe.stringPool),
		CompiledRuleCount: len(pe.rulesCache),
		ConfigEnabled:     pe.config != nil,
		UseCommonName:     pe.config != nil && pe.config.UseCommonName,
		UseSubjectAltName: pe.config != nil && pe.config.UseSubjectAltName,
		Normalize:         pe.config != nil && pe.config.Normalize,
	}
}

// ClearStringPool clears the string interning pool (useful for memory management)
func (pe *PrincipalExtractor) ClearStringPool() {
	pe.poolMutex.Lock()
	defer pe.poolMutex.Unlock()
	
	pe.stringPool = make(map[string]string)
}

// PrincipalExtractorStats represents extractor statistics
type PrincipalExtractorStats struct {
	StringPoolSize    int  `json:"string_pool_size"`
	CompiledRuleCount int  `json:"compiled_rule_count"`
	ConfigEnabled     bool `json:"config_enabled"`
	UseCommonName     bool `json:"use_common_name"`
	UseSubjectAltName bool `json:"use_subject_alt_name"`
	Normalize         bool `json:"normalize"`
}

// ValidatePrincipal validates a principal against common security patterns
func (pe *PrincipalExtractor) ValidatePrincipal(principal string) error {
	if principal == "" {
		return fmt.Errorf("principal cannot be empty")
	}
	
	// Check for common security issues
	if strings.Contains(principal, "..") {
		return fmt.Errorf("principal contains path traversal sequence")
	}
	
	if strings.ContainsAny(principal, "<>&\"'") {
		return fmt.Errorf("principal contains potentially dangerous characters")
	}
	
	// Check length limits
	if len(principal) > 256 {
		return fmt.Errorf("principal exceeds maximum length of 256 characters")
	}
	
	return nil
}

// ExtractPrincipalWithValidation extracts and validates a principal
func (pe *PrincipalExtractor) ExtractPrincipalWithValidation(certInfo *CertificateInfo) (string, error) {
	principal, err := pe.ExtractPrincipal(certInfo)
	if err != nil {
		return "", err
	}
	
	if err := pe.ValidatePrincipal(principal); err != nil {
		return "", fmt.Errorf("principal validation failed: %w", err)
	}
	
	return principal, nil
}