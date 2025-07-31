package rustmq

import (
	"context"
	"crypto/tls"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"github.com/quic-go/quic-go"
)

// Connection manages QUIC connections to RustMQ brokers
type Connection struct {
	config      *ClientConfig
	connections []quic.Connection
	current     int64
	mutex       sync.RWMutex
	stats       *ConnectionStats
}

// ConnectionStats holds connection statistics
type ConnectionStats struct {
	TotalConnections  int       `json:"total_connections"`
	ActiveConnections int       `json:"active_connections"`
	Brokers          []string  `json:"brokers"`
	BytesSent        uint64    `json:"bytes_sent"`
	BytesReceived    uint64    `json:"bytes_received"`
	RequestsSent     uint64    `json:"requests_sent"`
	ResponsesReceived uint64    `json:"responses_received"`
	Errors           uint64    `json:"errors"`
	LastHeartbeat    time.Time `json:"last_heartbeat"`
}

// newConnection creates a new connection to RustMQ brokers
func newConnection(config *ClientConfig) (*Connection, error) {
	conn := &Connection{
		config:      config,
		connections: make([]quic.Connection, 0, len(config.Brokers)),
		stats: &ConnectionStats{
			Brokers: config.Brokers,
		},
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
		tlsConfig = &tls.Config{
			InsecureSkipVerify: c.config.TLSConfig != nil && c.config.TLSConfig.InsecureSkipVerify,
		}
		
		if c.config.TLSConfig != nil && c.config.TLSConfig.ServerName != "" {
			tlsConfig.ServerName = c.config.TLSConfig.ServerName
		}
		
		// TODO: Add client certificate support
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
	defer stream.Close()

	// Send request
	_, err = stream.Write(request)
	if err != nil {
		atomic.AddUint64(&c.stats.Errors, 1)
		return nil, fmt.Errorf("failed to write request: %w", err)
	}

	atomic.AddUint64(&c.stats.BytesSent, uint64(len(request)))
	atomic.AddUint64(&c.stats.RequestsSent, 1)

	// Close write side to signal end of request
	stream.Close()

	// Read response
	response := make([]byte, 0, 4096)
	buffer := make([]byte, 1024)
	
	for {
		n, err := stream.Read(buffer)
		if err != nil {
			if err.Error() == "EOF" {
				break
			}
			atomic.AddUint64(&c.stats.Errors, 1)
			return nil, fmt.Errorf("failed to read response: %w", err)
		}
		response = append(response, buffer[:n]...)
	}

	atomic.AddUint64(&c.stats.BytesReceived, uint64(len(response)))
	atomic.AddUint64(&c.stats.ResponsesReceived, 1)

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
	if !c.IsConnected() {
		return fmt.Errorf("no active connections")
	}

	// TODO: Send actual health check request to broker
	c.stats.LastHeartbeat = time.Now()
	return nil
}

// reconnectBroker attempts to reconnect to a specific broker
func (c *Connection) reconnectBroker(index int) {
	if index >= len(c.config.Brokers) {
		return
	}

	broker := c.config.Brokers[index]
	
	// TODO: Implement reconnection logic
	fmt.Printf("Attempting to reconnect to broker %s\n", broker)
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
		TotalConnections:  c.stats.TotalConnections,
		ActiveConnections: c.stats.ActiveConnections,
		Brokers:          c.stats.Brokers,
		BytesSent:        atomic.LoadUint64(&c.stats.BytesSent),
		BytesReceived:    atomic.LoadUint64(&c.stats.BytesReceived),
		RequestsSent:     atomic.LoadUint64(&c.stats.RequestsSent),
		ResponsesReceived: atomic.LoadUint64(&c.stats.ResponsesReceived),
		Errors:           atomic.LoadUint64(&c.stats.Errors),
		LastHeartbeat:    c.stats.LastHeartbeat,
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
				// TODO: Implement reconnection logic
			}
		}
	}
}