package rustmq

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"math/rand"
	"sync"
	"sync/atomic"
	"time"

	"github.com/quic-go/quic-go"
)

// Connection manages QUIC connections to RustMQ brokers
type Connection struct {
	config       *ClientConfig
	connections  []quic.Connection
	current      int64
	mutex        sync.RWMutex
	stats        *ConnectionStats
	reconnecting map[int]bool
	backoffState map[int]*BackoffState
}

// ConnectionStats holds connection statistics
type ConnectionStats struct {
	TotalConnections   int         `json:"total_connections"`
	ActiveConnections  int         `json:"active_connections"`
	Brokers           []string     `json:"brokers"`
	BytesSent         uint64       `json:"bytes_sent"`
	BytesReceived     uint64       `json:"bytes_received"`
	RequestsSent      uint64       `json:"requests_sent"`
	ResponsesReceived uint64       `json:"responses_received"`
	Errors            uint64       `json:"errors"`
	LastHeartbeat     time.Time    `json:"last_heartbeat"`
	ReconnectAttempts uint64       `json:"reconnect_attempts"`
	HealthChecks      uint64       `json:"health_checks"`
	HealthCheckErrors uint64       `json:"health_check_errors"`
}

// BackoffState manages exponential backoff for reconnection attempts
type BackoffState struct {
	attempts    int
	lastAttempt time.Time
	backoff     time.Duration
}

// newConnection creates a new connection to RustMQ brokers
func newConnection(config *ClientConfig) (*Connection, error) {
	conn := &Connection{
		config:       config,
		connections:  make([]quic.Connection, 0, len(config.Brokers)),
		reconnecting: make(map[int]bool),
		backoffState: make(map[int]*BackoffState),
		stats: &ConnectionStats{
			Brokers: config.Brokers,
		},
	}

	// Initialize backoff states for each broker
	for i := range config.Brokers {
		conn.backoffState[i] = &BackoffState{
			backoff: config.RetryConfig.BaseDelay,
		}
	}

	// Establish connections to all brokers
	if err := conn.establishConnections(); err != nil {
		return nil, fmt.Errorf("failed to establish connections: %w", err)
	}

	return conn, nil
}

// establishConnections connects to all configured brokers
func (c *Connection) establishConnections() error {
	var tlsConfig *tls.Config
	if c.config.EnableTLS {
		var err error
		tlsConfig, err = c.buildTLSConfig()
		if err != nil {
			return fmt.Errorf("failed to build TLS config: %w", err)
		}
	}

	quicConfig := &quic.Config{
		HandshakeIdleTimeout:  c.config.ConnectTimeout,
		MaxIdleTimeout:        c.config.KeepAliveInterval,
		KeepAlivePeriod:       c.config.KeepAliveInterval / 2,
		EnableDatagrams:       true,
		MaxIncomingStreams:    100,
		MaxIncomingUniStreams: 100,
	}

	var lastErr error
	for _, broker := range c.config.Brokers {
		ctx, cancel := context.WithTimeout(context.Background(), c.config.ConnectTimeout)
		
		conn, err := quic.DialAddr(ctx, broker, tlsConfig, quicConfig)
		cancel()
		
		if err != nil {
			lastErr = err
			continue
		}

		c.connections = append(c.connections, conn)
	}

	if len(c.connections) == 0 {
		return fmt.Errorf("failed to connect to any broker: %w", lastErr)
	}

	c.stats.TotalConnections = len(c.connections)
	c.stats.ActiveConnections = len(c.connections)
	
	return nil
}

// getConnection returns the next available connection using round-robin
func (c *Connection) getConnection() (quic.Connection, error) {
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	if len(c.connections) == 0 {
		return nil, fmt.Errorf("no connections available")
	}

	// Round-robin selection
	index := atomic.AddInt64(&c.current, 1) % int64(len(c.connections))
	conn := c.connections[index]

	// Check if connection is still alive
	select {
	case <-conn.Context().Done():
		// Connection is closed, try to reconnect
		go c.reconnectBroker(int(index))
		return nil, fmt.Errorf("connection is closed")
	default:
		return conn, nil
	}
}

// sendRequest sends a request and waits for response
func (c *Connection) sendRequest(request []byte) ([]byte, error) {
	conn, err := c.getConnection()
	if err != nil {
		return nil, err
	}

	ctx, cancel := context.WithTimeout(context.Background(), c.config.RequestTimeout)
	defer cancel()

	// Open a new stream for this request
	stream, err := conn.OpenStreamSync(ctx)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		return nil, fmt.Errorf("failed to open stream: %w", err)
	}

	// Send request length first (4 bytes, big endian)
	lengthBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(lengthBytes, uint32(len(request)))
	_, err = stream.Write(lengthBytes)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		stream.Close()
		return nil, fmt.Errorf("failed to write request length: %w", err)
	}

	// Send request
	_, err = stream.Write(request)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		stream.Close()
		return nil, fmt.Errorf("failed to write request: %w", err)
	}

	atomic.AddUint64(&c.stats.BytesSent, uint64(len(request)+4))
	atomic.AddUint64(&c.stats.RequestsSent, 1)

	// Read response length first
	responseLengthBytes := make([]byte, 4)
	_, err = io.ReadFull(stream, responseLengthBytes)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		stream.Close()
		return nil, fmt.Errorf("failed to read response length: %w", err)
	}

	responseLength := binary.BigEndian.Uint32(responseLengthBytes)
	
	// Read response
	response := make([]byte, responseLength)
	_, err = io.ReadFull(stream, response)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		stream.Close()
		return nil, fmt.Errorf("failed to read response: %w", err)
	}

	atomic.AddUint64(&c.stats.BytesReceived, uint64(len(response)+4))
	atomic.AddUint64(&c.stats.ResponsesReceived, 1)
	
	stream.Close()
	return response, nil
}

// sendMessage sends a message without waiting for response
func (c *Connection) sendMessage(message []byte) error {
	conn, err := c.getConnection()
	if err != nil {
		return err
	}

	ctx, cancel := context.WithTimeout(context.Background(), c.config.RequestTimeout)
	defer cancel()

	// Open a new stream for this message
	stream, err := conn.OpenStreamSync(ctx)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		return fmt.Errorf("failed to open stream: %w", err)
	}
	defer stream.Close()

	// Send message
	_, err = stream.Write(message)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		return fmt.Errorf("failed to write message: %w", err)
	}

	atomic.AddUint64(&c.stats.BytesSent, uint64(len(message)))
	atomic.AddUint64(&c.stats.RequestsSent, 1)

	return nil
}

// IsConnected checks if there are active connections
func (c *Connection) IsConnected() bool {
	c.mutex.RLock()
	defer c.mutex.RUnlock()

	activeCount := 0
	for _, conn := range c.connections {
		select {
		case <-conn.Context().Done():
			// Connection is closed
		default:
			activeCount++
		}
	}

	c.stats.ActiveConnections = activeCount
	return activeCount > 0
}

// HealthCheck performs a health check on connections
func (c *Connection) HealthCheck() error {
	atomic.AddUint64(&c.stats.HealthChecks, 1)
	
	if !c.IsConnected() {
		atomic.AddUint64(&c.stats.HealthCheckErrors, 1)
		return fmt.Errorf("no active connections")
	}

	// Send actual health check request to broker
	healthCheckMsg := struct {
		Type      string    `json:"type"`
		Timestamp time.Time `json:"timestamp"`
		ClientID  string    `json:"client_id"`
	}{
		Type:      "health_check",
		Timestamp: time.Now(),
		ClientID:  c.config.ClientID,
	}

	data, err := json.Marshal(healthCheckMsg)
	if err != nil {
		atomic.AddUint64(&c.stats.HealthCheckErrors, 1)
		return fmt.Errorf("failed to marshal health check message: %w", err)
	}

	response, err := c.sendRequest(data)
	if err != nil {
		atomic.AddUint64(&c.stats.HealthCheckErrors, 1)
		return fmt.Errorf("health check request failed: %w", err)
	}

	// Parse health check response
	var healthResponse HealthCheck
	if err := json.Unmarshal(response, &healthResponse); err != nil {
		atomic.AddUint64(&c.stats.HealthCheckErrors, 1)
		return fmt.Errorf("failed to parse health check response: %w", err)
	}

	// Check if the broker reports as healthy
	if healthResponse.Status != HealthStatusHealthy {
		atomic.AddUint64(&c.stats.HealthCheckErrors, 1)
		return fmt.Errorf("broker reports unhealthy status: %s", healthResponse.Status)
	}

	c.stats.LastHeartbeat = time.Now()
	return nil
}

// reconnectBroker attempts to reconnect to a specific broker
func (c *Connection) reconnectBroker(index int) {
	if index >= len(c.config.Brokers) {
		return
	}

	c.mutex.Lock()
	// Check if already reconnecting
	if c.reconnecting[index] {
		c.mutex.Unlock()
		return
	}
	c.reconnecting[index] = true
	c.mutex.Unlock()

	defer func() {
		c.mutex.Lock()
		c.reconnecting[index] = false
		c.mutex.Unlock()
	}()

	broker := c.config.Brokers[index]
	backoff := c.backoffState[index]
	
	// Check if we should wait for backoff
	if time.Since(backoff.lastAttempt) < backoff.backoff {
		return
	}

	atomic.AddUint64(&c.stats.ReconnectAttempts, 1)
	backoff.attempts++
	backoff.lastAttempt = time.Now()

	// Build TLS config
	var tlsConfig *tls.Config
	var err error
	if c.config.EnableTLS {
		tlsConfig, err = c.buildTLSConfig()
		if err != nil {
			fmt.Printf("Failed to build TLS config for reconnection to %s: %v\n", broker, err)
			c.updateBackoff(index)
			return
		}
	}

	// Build QUIC config
	quicConfig := &quic.Config{
		HandshakeIdleTimeout:  c.config.ConnectTimeout,
		MaxIdleTimeout:        c.config.KeepAliveInterval,
		KeepAlivePeriod:       c.config.KeepAliveInterval / 2,
		EnableDatagrams:       true,
		MaxIncomingStreams:    100,
		MaxIncomingUniStreams: 100,
	}

	// Attempt to connect
	ctx, cancel := context.WithTimeout(context.Background(), c.config.ConnectTimeout)
	defer cancel()

	conn, err := quic.DialAddr(ctx, broker, tlsConfig, quicConfig)
	if err != nil {
		fmt.Printf("Reconnection to broker %s failed: %v\n", broker, err)
		c.updateBackoff(index)
		return
	}

	// Replace the old connection
	c.mutex.Lock()
	if index < len(c.connections) {
		if c.connections[index] != nil {
			c.connections[index].CloseWithError(0, "replacing connection")
		}
		c.connections[index] = conn
	}
	c.mutex.Unlock()

	// Reset backoff on successful connection
	backoff.attempts = 0
	backoff.backoff = c.config.RetryConfig.BaseDelay

	fmt.Printf("Successfully reconnected to broker %s\n", broker)
}

// reconnect reconnects to all brokers
func (c *Connection) reconnect() error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	// Close existing connections
	for _, conn := range c.connections {
		conn.CloseWithError(0, "reconnecting")
	}

	c.connections = c.connections[:0]

	// Re-establish connections
	return c.establishConnections()
}

// Stats returns current connection statistics
func (c *Connection) Stats() *ConnectionStats {
	// Update active connections count
	c.IsConnected()
	
	// Return a copy of stats
	return &ConnectionStats{
		TotalConnections:   c.stats.TotalConnections,
		ActiveConnections:  c.stats.ActiveConnections,
		Brokers:           c.stats.Brokers,
		BytesSent:         atomic.LoadUint64(&c.stats.BytesSent),
		BytesReceived:     atomic.LoadUint64(&c.stats.BytesReceived),
		RequestsSent:      atomic.LoadUint64(&c.stats.RequestsSent),
		ResponsesReceived: atomic.LoadUint64(&c.stats.ResponsesReceived),
		Errors:            atomic.LoadUint64(&c.stats.Errors),
		LastHeartbeat:     c.stats.LastHeartbeat,
		ReconnectAttempts: atomic.LoadUint64(&c.stats.ReconnectAttempts),
		HealthChecks:      atomic.LoadUint64(&c.stats.HealthChecks),
		HealthCheckErrors: atomic.LoadUint64(&c.stats.HealthCheckErrors),
	}
}

// Close closes all connections
func (c *Connection) Close() error {
	c.mutex.Lock()
	defer c.mutex.Unlock()

	for _, conn := range c.connections {
		conn.CloseWithError(0, "client closing")
	}

	c.connections = c.connections[:0]
	c.stats.ActiveConnections = 0

	return nil
}

// buildTLSConfig constructs a TLS configuration with client certificate support
func (c *Connection) buildTLSConfig() (*tls.Config, error) {
	tlsConfig := &tls.Config{
		InsecureSkipVerify: c.config.TLSConfig != nil && c.config.TLSConfig.InsecureSkipVerify,
	}

	if c.config.TLSConfig == nil {
		return tlsConfig, nil
	}

	// Set server name
	if c.config.TLSConfig.ServerName != "" {
		tlsConfig.ServerName = c.config.TLSConfig.ServerName
	}

	// Load CA certificate if provided
	if c.config.TLSConfig.CACert != "" {
		caCert, err := ioutil.ReadFile(c.config.TLSConfig.CACert)
		if err != nil {
			return nil, fmt.Errorf("failed to read CA certificate: %w", err)
		}

		caCertPool := x509.NewCertPool()
		if !caCertPool.AppendCertsFromPEM(caCert) {
			return nil, fmt.Errorf("failed to parse CA certificate")
		}
		tlsConfig.RootCAs = caCertPool
	}

	// Load client certificate and key if provided
	if c.config.TLSConfig.ClientCert != "" && c.config.TLSConfig.ClientKey != "" {
		clientCert, err := tls.LoadX509KeyPair(c.config.TLSConfig.ClientCert, c.config.TLSConfig.ClientKey)
		if err != nil {
			return nil, fmt.Errorf("failed to load client certificate: %w", err)
		}
		tlsConfig.Certificates = []tls.Certificate{clientCert}
	}

	return tlsConfig, nil
}

// updateBackoff updates the backoff duration for a broker connection
func (c *Connection) updateBackoff(index int) {
	backoff := c.backoffState[index]
	if backoff.attempts >= c.config.RetryConfig.MaxRetries {
		// Max retries reached, set to max delay
		backoff.backoff = c.config.RetryConfig.MaxDelay
		return
	}

	// Exponential backoff with optional jitter
	newBackoff := time.Duration(float64(backoff.backoff) * c.config.RetryConfig.Multiplier)
	if newBackoff > c.config.RetryConfig.MaxDelay {
		newBackoff = c.config.RetryConfig.MaxDelay
	}

	// Add jitter if enabled
	if c.config.RetryConfig.Jitter {
		jitter := time.Duration(rand.Float64() * float64(newBackoff) * 0.1) // 10% jitter
		newBackoff += jitter
	}

	backoff.backoff = newBackoff
}

// HealthChecker manages connection health monitoring
type HealthChecker struct {
	client   *Client
	interval time.Duration
}

// newHealthChecker creates a new health checker
func newHealthChecker(client *Client) *HealthChecker {
	return &HealthChecker{
		client:   client,
		interval: client.config.KeepAliveInterval,
	}
}

// start begins health checking
func (h *HealthChecker) start(ctx context.Context) {
	ticker := time.NewTicker(h.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if err := h.client.HealthCheck(); err != nil {
				fmt.Printf("Health check failed: %v\n", err)
				
				// Trigger reconnection for failed connections
				connection := h.client.GetConnection()
				connection.mutex.RLock()
				for i, conn := range connection.connections {
					select {
					case <-conn.Context().Done():
						// Connection is closed, trigger reconnection
						go connection.reconnectBroker(i)
					default:
						// Connection appears active, but health check failed
						// This might indicate a network issue, so we also try to reconnect
						if !connection.reconnecting[i] {
							go connection.reconnectBroker(i)
						}
					}
				}
				connection.mutex.RUnlock()
			}
		}
	}
}