package rustmq

import (
	"fmt"
)

// SecurityError represents security-related errors
type SecurityError struct {
	Type     SecurityErrorType `json:"type"`
	Message  string            `json:"message"`
	Code     string            `json:"code,omitempty"`
	Details  map[string]interface{} `json:"details,omitempty"`
	Cause    error             `json:"-"`
}

// SecurityErrorType represents types of security errors
type SecurityErrorType string

const (
	// Authentication errors
	SecurityErrorAuthenticationFailed SecurityErrorType = "authentication_failed"
	SecurityErrorInvalidCredentials   SecurityErrorType = "invalid_credentials"
	SecurityErrorTokenExpired         SecurityErrorType = "token_expired"
	SecurityErrorTokenInvalid         SecurityErrorType = "token_invalid"
	SecurityErrorMTLSFailed          SecurityErrorType = "mtls_failed"
	SecurityErrorSASLFailed          SecurityErrorType = "sasl_failed"
	
	// Authorization errors
	SecurityErrorAccessDenied        SecurityErrorType = "access_denied"
	SecurityErrorInsufficientRights  SecurityErrorType = "insufficient_rights"
	SecurityErrorACLCheckFailed      SecurityErrorType = "acl_check_failed"
	SecurityErrorPermissionDenied    SecurityErrorType = "permission_denied"
	
	// Certificate errors
	SecurityErrorInvalidCertificate   SecurityErrorType = "invalid_certificate"
	SecurityErrorCertificateExpired   SecurityErrorType = "certificate_expired"
	SecurityErrorCertificateRevoked   SecurityErrorType = "certificate_revoked"
	SecurityErrorCertificateValidation SecurityErrorType = "certificate_validation"
	SecurityErrorCAValidation        SecurityErrorType = "ca_validation"
	
	// TLS errors
	SecurityErrorTLSHandshake        SecurityErrorType = "tls_handshake"
	SecurityErrorTLSConfiguration    SecurityErrorType = "tls_configuration"
	SecurityErrorTLSConnection       SecurityErrorType = "tls_connection"
	
	// Configuration errors
	SecurityErrorInvalidConfig       SecurityErrorType = "invalid_config"
	SecurityErrorMissingConfig       SecurityErrorType = "missing_config"
	SecurityErrorConfigValidation    SecurityErrorType = "config_validation"
	
	// Service errors
	SecurityErrorServiceUnavailable  SecurityErrorType = "service_unavailable"
	SecurityErrorCircuitBreakerOpen  SecurityErrorType = "circuit_breaker_open"
	SecurityErrorTimeout            SecurityErrorType = "timeout"
	SecurityErrorRateLimited        SecurityErrorType = "rate_limited"
	
	// Internal errors
	SecurityErrorInternal           SecurityErrorType = "internal_error"
	SecurityErrorUnknown            SecurityErrorType = "unknown_error"
)

// Error implements the error interface
func (e *SecurityError) Error() string {
	if e.Cause != nil {
		return fmt.Sprintf("%s: %s (caused by: %v)", e.Type, e.Message, e.Cause)
	}
	return fmt.Sprintf("%s: %s", e.Type, e.Message)
}

// Unwrap returns the underlying cause
func (e *SecurityError) Unwrap() error {
	return e.Cause
}

// IsType checks if the error is of a specific type
func (e *SecurityError) IsType(errorType SecurityErrorType) bool {
	return e.Type == errorType
}

// WithDetails adds details to the error
func (e *SecurityError) WithDetails(key string, value interface{}) *SecurityError {
	if e.Details == nil {
		e.Details = make(map[string]interface{})
	}
	e.Details[key] = value
	return e
}

// WithCode adds an error code
func (e *SecurityError) WithCode(code string) *SecurityError {
	e.Code = code
	return e
}

// NewSecurityError creates a new security error
func NewSecurityError(errorType SecurityErrorType, message string) *SecurityError {
	return &SecurityError{
		Type:    errorType,
		Message: message,
		Details: make(map[string]interface{}),
	}
}

// NewSecurityErrorWithCause creates a new security error with an underlying cause
func NewSecurityErrorWithCause(errorType SecurityErrorType, message string, cause error) *SecurityError {
	return &SecurityError{
		Type:    errorType,
		Message: message,
		Cause:   cause,
		Details: make(map[string]interface{}),
	}
}

// Authentication error constructors
func NewAuthenticationFailedError(message string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorAuthenticationFailed, message, cause)
}

func NewInvalidCredentialsError(message string) *SecurityError {
	return NewSecurityError(SecurityErrorInvalidCredentials, message)
}

func NewTokenExpiredError(message string) *SecurityError {
	return NewSecurityError(SecurityErrorTokenExpired, message)
}

func NewTokenInvalidError(message string) *SecurityError {
	return NewSecurityError(SecurityErrorTokenInvalid, message)
}

func NewMTLSFailedError(message string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorMTLSFailed, message, cause)
}

func NewSASLFailedError(message string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorSASLFailed, message, cause)
}

// Authorization error constructors
func NewAccessDeniedError(resource, operation string) *SecurityError {
	return NewSecurityError(SecurityErrorAccessDenied, 
		fmt.Sprintf("access denied for operation '%s' on resource '%s'", operation, resource)).
		WithDetails("resource", resource).
		WithDetails("operation", operation)
}

func NewInsufficientRightsError(required, actual string) *SecurityError {
	return NewSecurityError(SecurityErrorInsufficientRights,
		fmt.Sprintf("insufficient rights: required '%s', have '%s'", required, actual)).
		WithDetails("required", required).
		WithDetails("actual", actual)
}

func NewACLCheckFailedError(principal string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorACLCheckFailed,
		fmt.Sprintf("ACL check failed for principal '%s'", principal), cause).
		WithDetails("principal", principal)
}

func NewPermissionDeniedError(topic, operation, principal string) *SecurityError {
	return NewSecurityError(SecurityErrorPermissionDenied,
		fmt.Sprintf("permission denied: principal '%s' cannot '%s' on topic '%s'", 
			principal, operation, topic)).
		WithDetails("topic", topic).
		WithDetails("operation", operation).
		WithDetails("principal", principal)
}

// Certificate error constructors
func NewInvalidCertificateError(reason string) *SecurityError {
	return NewSecurityError(SecurityErrorInvalidCertificate, reason)
}

func NewCertificateExpiredError(notAfter string) *SecurityError {
	return NewSecurityError(SecurityErrorCertificateExpired,
		fmt.Sprintf("certificate expired at %s", notAfter)).
		WithDetails("not_after", notAfter)
}

func NewCertificateRevokedError(reason string) *SecurityError {
	return NewSecurityError(SecurityErrorCertificateRevoked, reason)
}

func NewCertificateValidationError(reason string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorCertificateValidation, reason, cause)
}

func NewCAValidationError(reason string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorCAValidation, reason, cause)
}

// TLS error constructors
func NewTLSHandshakeError(reason string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorTLSHandshake, reason, cause)
}

func NewTLSConfigurationError(reason string) *SecurityError {
	return NewSecurityError(SecurityErrorTLSConfiguration, reason)
}

func NewTLSConnectionError(reason string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorTLSConnection, reason, cause)
}

// Configuration error constructors
func NewInvalidConfigError(field, reason string) *SecurityError {
	return NewSecurityError(SecurityErrorInvalidConfig,
		fmt.Sprintf("invalid configuration for field '%s': %s", field, reason)).
		WithDetails("field", field).
		WithDetails("reason", reason)
}

func NewMissingConfigError(field string) *SecurityError {
	return NewSecurityError(SecurityErrorMissingConfig,
		fmt.Sprintf("missing required configuration: %s", field)).
		WithDetails("field", field)
}

func NewConfigValidationError(reason string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorConfigValidation, reason, cause)
}

// Service error constructors
func NewServiceUnavailableError(service string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorServiceUnavailable,
		fmt.Sprintf("service '%s' unavailable", service), cause).
		WithDetails("service", service)
}

func NewCircuitBreakerOpenError(service string) *SecurityError {
	return NewSecurityError(SecurityErrorCircuitBreakerOpen,
		fmt.Sprintf("circuit breaker open for service '%s'", service)).
		WithDetails("service", service)
}

func NewTimeoutError(operation string, timeout string) *SecurityError {
	return NewSecurityError(SecurityErrorTimeout,
		fmt.Sprintf("timeout during '%s' after %s", operation, timeout)).
		WithDetails("operation", operation).
		WithDetails("timeout", timeout)
}

func NewRateLimitedError(resource string, limit string) *SecurityError {
	return NewSecurityError(SecurityErrorRateLimited,
		fmt.Sprintf("rate limited for resource '%s' (limit: %s)", resource, limit)).
		WithDetails("resource", resource).
		WithDetails("limit", limit)
}

// Internal error constructors
func NewInternalError(message string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorInternal, message, cause)
}

func NewUnknownError(message string, cause error) *SecurityError {
	return NewSecurityErrorWithCause(SecurityErrorUnknown, message, cause)
}

// IsSecurityError checks if an error is a security error
func IsSecurityError(err error) bool {
	_, ok := err.(*SecurityError)
	return ok
}

// AsSecurityError converts an error to a security error if possible
func AsSecurityError(err error) (*SecurityError, bool) {
	if secErr, ok := err.(*SecurityError); ok {
		return secErr, true
	}
	return nil, false
}

// IsAuthenticationError checks if an error is an authentication error
func IsAuthenticationError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorAuthenticationFailed,
			 SecurityErrorInvalidCredentials,
			 SecurityErrorTokenExpired,
			 SecurityErrorTokenInvalid,
			 SecurityErrorMTLSFailed,
			 SecurityErrorSASLFailed:
			return true
		}
	}
	return false
}

// IsAuthorizationError checks if an error is an authorization error
func IsAuthorizationError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorAccessDenied,
			 SecurityErrorInsufficientRights,
			 SecurityErrorACLCheckFailed,
			 SecurityErrorPermissionDenied:
			return true
		}
	}
	return false
}

// IsCertificateError checks if an error is a certificate error
func IsCertificateError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorInvalidCertificate,
			 SecurityErrorCertificateExpired,
			 SecurityErrorCertificateRevoked,
			 SecurityErrorCertificateValidation,
			 SecurityErrorCAValidation:
			return true
		}
	}
	return false
}

// IsTLSError checks if an error is a TLS error
func IsTLSError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorTLSHandshake,
			 SecurityErrorTLSConfiguration,
			 SecurityErrorTLSConnection:
			return true
		}
	}
	return false
}

// IsConfigurationError checks if an error is a configuration error
func IsConfigurationError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorInvalidConfig,
			 SecurityErrorMissingConfig,
			 SecurityErrorConfigValidation:
			return true
		}
	}
	return false
}

// IsServiceError checks if an error is a service error
func IsServiceError(err error) bool {
	if secErr, ok := AsSecurityError(err); ok {
		switch secErr.Type {
		case SecurityErrorServiceUnavailable,
			 SecurityErrorCircuitBreakerOpen,
			 SecurityErrorTimeout,
			 SecurityErrorRateLimited:
			return true
		}
	}
	return false
}

// ErrorSummary provides a summary of security errors
type ErrorSummary struct {
	TotalErrors        int                           `json:"total_errors"`
	ErrorsByType       map[SecurityErrorType]int     `json:"errors_by_type"`
	AuthenticationRate float64                       `json:"authentication_error_rate"`
	AuthorizationRate  float64                       `json:"authorization_error_rate"`
	CertificateRate    float64                       `json:"certificate_error_rate"`
	TLSRate           float64                       `json:"tls_error_rate"`
	ConfigurationRate  float64                       `json:"configuration_error_rate"`
	ServiceRate       float64                       `json:"service_error_rate"`
	MostCommonError   SecurityErrorType             `json:"most_common_error"`
}

// CreateErrorSummary creates an error summary from a list of errors
func CreateErrorSummary(errors []*SecurityError) *ErrorSummary {
	if len(errors) == 0 {
		return &ErrorSummary{
			ErrorsByType: make(map[SecurityErrorType]int),
		}
	}
	
	summary := &ErrorSummary{
		TotalErrors:  len(errors),
		ErrorsByType: make(map[SecurityErrorType]int),
	}
	
	authErrors := 0
	authzErrors := 0
	certErrors := 0
	tlsErrors := 0
	configErrors := 0
	serviceErrors := 0
	
	maxCount := 0
	var mostCommon SecurityErrorType
	
	for _, err := range errors {
		summary.ErrorsByType[err.Type]++
		
		if summary.ErrorsByType[err.Type] > maxCount {
			maxCount = summary.ErrorsByType[err.Type]
			mostCommon = err.Type
		}
		
		if IsAuthenticationError(err) {
			authErrors++
		} else if IsAuthorizationError(err) {
			authzErrors++
		} else if IsCertificateError(err) {
			certErrors++
		} else if IsTLSError(err) {
			tlsErrors++
		} else if IsConfigurationError(err) {
			configErrors++
		} else if IsServiceError(err) {
			serviceErrors++
		}
	}
	
	total := float64(summary.TotalErrors)
	summary.AuthenticationRate = float64(authErrors) / total
	summary.AuthorizationRate = float64(authzErrors) / total
	summary.CertificateRate = float64(certErrors) / total
	summary.TLSRate = float64(tlsErrors) / total
	summary.ConfigurationRate = float64(configErrors) / total
	summary.ServiceRate = float64(serviceErrors) / total
	summary.MostCommonError = mostCommon
	
	return summary
}