// Package rustmq provides a high-performance Go client for RustMQ message queue system
package rustmq

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/google/uuid"
)

// Client represents the main RustMQ client for managing connections and creating producers/consumers
type Client struct {
	config          *ClientConfig
	connection      *Connection
	producers       map[string]*Producer
	consumers       map[string]*Consumer
	securityManager *SecurityManager
	securityContext *SecurityContext
	mutex           sync.RWMutex
	ctx             context.Context
	cancel          context.CancelFunc
	healthCheck     *HealthChecker
}

// ClientConfig holds configuration for the RustMQ client
type ClientConfig struct {
	// Broker endpoints
	Brokers []string `json:"brokers"`
	
	// Client ID for identification
	ClientID string `json:"client_id,omitempty"`
	
	// Connection timeout
	ConnectTimeout time.Duration `json:"connect_timeout"`
	
	// Request timeout
	RequestTimeout time.Duration `json:"request_timeout"`
	
	// Enable TLS
	EnableTLS bool `json:"enable_tls"`
	
	// TLS configuration
	TLSConfig *TLSConfig `json:"tls_config,omitempty"`
	
	// Connection pool size
	MaxConnections int `json:"max_connections"`
	
	// Keep-alive interval
	KeepAliveInterval time.Duration `json:"keep_alive_interval"`
	
	// Retry configuration
	RetryConfig *RetryConfig `json:"retry_config"`
	
	// Compression settings
	Compression *CompressionConfig `json:"compression"`
	
	// Authentication settings (deprecated - use Security.Auth instead)
	Auth *AuthConfig `json:"auth,omitempty"`
	
	// Security configuration
	Security *SecurityConfig `json:"security,omitempty"`
}

// TLSConfig holds TLS configuration
type TLSConfig struct {
	CACert               string `json:"ca_cert,omitempty"`
	ClientCert           string `json:"client_cert,omitempty"`
	ClientKey            string `json:"client_key,omitempty"`
	ServerName           string `json:"server_name,omitempty"`
	InsecureSkipVerify   bool   `json:"insecure_skip_verify"`
}

// RetryConfig holds retry configuration
type RetryConfig struct {
	MaxRetries int           `json:"max_retries"`
	BaseDelay  time.Duration `json:"base_delay"`
	MaxDelay   time.Duration `json:"max_delay"`
	Multiplier float64       `json:"multiplier"`
	Jitter     bool          `json:"jitter"`
}

// CompressionConfig holds compression settings
type CompressionConfig struct {
	Enabled   bool                `json:"enabled"`
	Algorithm CompressionAlgorithm `json:"algorithm"`
	Level     int                 `json:"level"`
	MinSize   int                 `json:"min_size"`
}

// CompressionAlgorithm represents compression algorithms
type CompressionAlgorithm string

const (
	CompressionNone CompressionAlgorithm = "none"
	CompressionGzip CompressionAlgorithm = "gzip"
	CompressionLZ4  CompressionAlgorithm = "lz4"
	CompressionZstd CompressionAlgorithm = "zstd"
)

// AuthConfig holds authentication configuration
type AuthConfig struct {
	Method     AuthMethod        `json:"method"`
	Username   string            `json:"username,omitempty"`
	Password   string            `json:"password,omitempty"`
	Token      string            `json:"token,omitempty"`
	Properties map[string]string `json:"properties,omitempty"`
}

// AuthMethod represents authentication methods
type AuthMethod string

const (
	AuthNone        AuthMethod = "none"
	AuthSASLPlain   AuthMethod = "sasl_plain"
	AuthSASLScram256 AuthMethod = "sasl_scram_256"
	AuthSASLScram512 AuthMethod = "sasl_scram_512"
	AuthToken       AuthMethod = "token"
	AuthMTLS        AuthMethod = "mtls"
)

// DefaultClientConfig returns a default client configuration
func DefaultClientConfig() *ClientConfig {
	return &ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          fmt.Sprintf("rustmq-go-client-%s", uuid.New().String()[:8]),
		ConnectTimeout:    10 * time.Second,
		RequestTimeout:    30 * time.Second,
		EnableTLS:         false,
		MaxConnections:    10,
		KeepAliveInterval: 30 * time.Second,
		RetryConfig: &RetryConfig{
			MaxRetries: 3,
			BaseDelay:  100 * time.Millisecond,
			MaxDelay:   10 * time.Second,
			Multiplier: 2.0,
			Jitter:     true,
		},
		Compression: &CompressionConfig{
			Enabled:   false,
			Algorithm: CompressionNone,
			Level:     6,
			MinSize:   1024,
		},
	}
}

// NewClient creates a new RustMQ client
func NewClient(config *ClientConfig) (*Client, error) {
	if config == nil {
		config = DefaultClientConfig()
	}

	ctx, cancel := context.WithCancel(context.Background())

	client := &Client{
		config:    config,
		producers: make(map[string]*Producer),
		consumers: make(map[string]*Consumer),
		ctx:       ctx,
		cancel:    cancel,
	}

	// Initialize security manager if security is configured
	if err := client.initializeSecurity(); err != nil {
		cancel()
		return nil, fmt.Errorf("failed to initialize security: %w", err)
	}

	// Create connection with security context
	connection, err := newConnectionWithSecurity(config, client.securityManager)
	if err != nil {
		cancel()
		if client.securityManager != nil {
			client.securityManager.Close()
		}
		return nil, fmt.Errorf("failed to create connection: %w", err)
	}
	client.connection = connection

	// Start health checker
	client.healthCheck = newHealthChecker(client)
	go client.healthCheck.start(ctx)

	return client, nil
}

// CreateProducer creates a new producer for the specified topic
func (c *Client) CreateProducer(topic string, config ...*ProducerConfig) (*Producer, error) {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Check if producer already exists
	if producer, exists := c.producers[topic]; exists {
		return producer, nil
	}

	var producerConfig *ProducerConfig
	if len(config) > 0 {
		producerConfig = config[0]
	} else {
		producerConfig = DefaultProducerConfig()
	}

	producer, err := newProducer(c, topic, producerConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create producer: %w", err)
	}

	c.producers[topic] = producer
	return producer, nil
}

// CreateConsumer creates a new consumer for the specified topic and group
func (c *Client) CreateConsumer(topic, consumerGroup string, config ...*ConsumerConfig) (*Consumer, error) {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	key := fmt.Sprintf("%s:%s", topic, consumerGroup)
	
	// Check if consumer already exists
	if consumer, exists := c.consumers[key]; exists {
		return consumer, nil
	}

	var consumerConfig *ConsumerConfig
	if len(config) > 0 {
		consumerConfig = config[0]
	} else {
		consumerConfig = DefaultConsumerConfig()
		consumerConfig.ConsumerGroup = consumerGroup
	}

	consumer, err := newConsumer(c, topic, consumerGroup, consumerConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create consumer: %w", err)
	}

	c.consumers[key] = consumer
	return consumer, nil
}

// CreateStream creates a streaming interface for real-time message processing
func (c *Client) CreateStream(config *StreamConfig) (*MessageStream, error) {
	return newMessageStream(c, config)
}

// Producer returns an existing producer for the topic or creates a new one
func (c *Client) Producer(topic string) (*Producer, error) {
	c.mutex.RLock()
	if producer, exists := c.producers[topic]; exists {
		c.mutex.RUnlock()
		return producer, nil
	}
	c.mutex.RUnlock()

	return c.CreateProducer(topic)
}

// Consumer returns an existing consumer or creates a new one
func (c *Client) Consumer(topic, consumerGroup string) (*Consumer, error) {
	key := fmt.Sprintf("%s:%s", topic, consumerGroup)
	
	c.mutex.RLock()
	if consumer, exists := c.consumers[key]; exists {
		c.mutex.RUnlock()
		return consumer, nil
	}
	c.mutex.RUnlock()

	return c.CreateConsumer(topic, consumerGroup)
}

// IsConnected checks if the client is connected to brokers
func (c *Client) IsConnected() bool {
	return c.connection.IsConnected()
}

// HealthCheck performs a health check on the client
func (c *Client) HealthCheck() error {
	return c.connection.HealthCheck()
}

// Stats returns connection statistics
func (c *Client) Stats() *ConnectionStats {
	return c.connection.Stats()
}

// Close closes the client and all associated resources
func (c *Client) Close() error {
	c.cancel()

	// Close all producers
	c.mutex.Lock()
	for _, producer := range c.producers {
		if err := producer.Close(); err != nil {
			// Log error but continue closing other resources
			fmt.Printf("Error closing producer: %v\n", err)
		}
	}

	// Close all consumers
	for _, consumer := range c.consumers {
		if err := consumer.Close(); err != nil {
			// Log error but continue closing other resources
			fmt.Printf("Error closing consumer: %v\n", err)
		}
	}
	c.mutex.Unlock()

	// Close security manager
	if c.securityManager != nil {
		if err := c.securityManager.Close(); err != nil {
			fmt.Printf("Error closing security manager: %v\n", err)
		}
	}

	// Close connection
	return c.connection.Close()
}

// Config returns the client configuration
func (c *Client) Config() *ClientConfig {
	return c.config
}

// GetConnection returns the underlying connection (internal use)
func (c *Client) GetConnection() *Connection {
	return c.connection
}

// Context returns the client context
func (c *Client) Context() context.Context {
	return c.ctx
}

// initializeSecurity initializes the security manager and performs authentication
func (c *Client) initializeSecurity() error {
	// Merge legacy auth config with new security config for backward compatibility
	securityConfig := c.mergeSecurityConfig()
	
	if securityConfig == nil {
		return nil // No security configured
	}
	
	// Create security manager
	securityManager, err := NewSecurityManager(securityConfig)
	if err != nil {
		return fmt.Errorf("failed to create security manager: %w", err)
	}
	c.securityManager = securityManager
	
	// Perform authentication
	securityContext, err := securityManager.Authenticate(c.ctx)
	if err != nil {
		return fmt.Errorf("authentication failed: %w", err)
	}
	c.securityContext = securityContext
	
	return nil
}

// mergeSecurityConfig merges legacy auth config with new security config
func (c *Client) mergeSecurityConfig() *SecurityConfig {
	var securityConfig *SecurityConfig
	
	// Start with new security config if provided
	if c.config.Security != nil {
		securityConfig = c.config.Security
	} else if c.config.Auth != nil {
		// Create security config from legacy auth config
		securityConfig = &SecurityConfig{
			Auth: &AuthenticationConfig{
				Method:     AuthenticationMethod(c.config.Auth.Method),
				Username:   c.config.Auth.Username,
				Password:   c.config.Auth.Password,
				Token:      c.config.Auth.Token,
				Properties: c.config.Auth.Properties,
			},
		}
	}
	
	// Configure TLS security if TLS is enabled
	if c.config.EnableTLS && c.config.TLSConfig != nil {
		if securityConfig == nil {
			securityConfig = DefaultSecurityConfig()
		}
		
		if securityConfig.TLS == nil {
			securityConfig.TLS = &TLSSecurityConfig{}
		}
		
		// Map legacy TLS config to security TLS config
		securityConfig.TLS.Mode = TLSModeEnabled
		securityConfig.TLS.CACert = c.config.TLSConfig.CACert
		securityConfig.TLS.ClientCert = c.config.TLSConfig.ClientCert
		securityConfig.TLS.ClientKey = c.config.TLSConfig.ClientKey
		securityConfig.TLS.ServerName = c.config.TLSConfig.ServerName
		securityConfig.TLS.InsecureSkipVerify = c.config.TLSConfig.InsecureSkipVerify
		
		// Enable mTLS if client certificate is provided
		if securityConfig.TLS.ClientCert != "" && securityConfig.TLS.ClientKey != "" {
			securityConfig.TLS.Mode = TLSModeMutualAuth
			if securityConfig.Auth == nil {
				securityConfig.Auth = &AuthenticationConfig{Method: AuthMethodMTLS}
			}
		}
	}
	
	return securityConfig
}

// SecurityContext returns the current security context
func (c *Client) SecurityContext() *SecurityContext {
	c.mutex.RLock()
	defer c.mutex.RUnlock()
	return c.securityContext
}

// SecurityManager returns the security manager
func (c *Client) SecurityManager() *SecurityManager {
	return c.securityManager
}

// RefreshSecurityContext refreshes the security context
func (c *Client) RefreshSecurityContext() error {
	if c.securityManager == nil {
		return fmt.Errorf("security manager not initialized")
	}
	
	securityContext, err := c.securityManager.Authenticate(c.ctx)
	if err != nil {
		return fmt.Errorf("failed to refresh security context: %w", err)
	}
	
	c.mutex.Lock()
	c.securityContext = securityContext
	c.mutex.Unlock()
	
	return nil
}

// IsSecured returns true if the client has security enabled
func (c *Client) IsSecured() bool {
	return c.securityManager != nil
}

// CanReadTopic checks if the client can read from a topic
func (c *Client) CanReadTopic(topic string) bool {
	if c.securityContext == nil || c.securityContext.Permissions == nil {
		return true // Allow if no security context
	}
	return c.securityContext.Permissions.CanReadTopic(topic)
}

// CanWriteTopic checks if the client can write to a topic  
func (c *Client) CanWriteTopic(topic string) bool {
	if c.securityContext == nil || c.securityContext.Permissions == nil {
		return true // Allow if no security context
	}
	return c.securityContext.Permissions.CanWriteTopic(topic)
}

// CanPerformAdminOperation checks if the client can perform admin operations
func (c *Client) CanPerformAdminOperation(operation string) bool {
	if c.securityContext == nil || c.securityContext.Permissions == nil {
		return true // Allow if no security context
	}
	return c.securityContext.Permissions.CanPerformAdminOperation(operation)
}