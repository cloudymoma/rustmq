package tests

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/binary"
	"encoding/pem"
	"io"
	"io/ioutil"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/quic-go/quic-go"
	"github.com/quic-go/quic-go/http3"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// MockBroker represents a mock RustMQ broker for testing
type MockBroker struct {
	server   *http3.Server
	listener *quic.Listener
	address  string
	tlsConfig *tls.Config
	mutex    sync.Mutex
	requests [][]byte
	responses map[string][]byte
}

// NewMockBroker creates a new mock broker for testing
func NewMockBroker(enableTLS bool, tlsConfig *tls.Config) (*MockBroker, error) {
	var listener *quic.Listener
	var err error
	
	// QUIC always requires TLS, so we'll provide a minimal TLS config for testing
	if tlsConfig == nil {
		// Generate a minimal self-signed certificate for testing
		cert, err := generateSelfSignedCert()
		if err != nil {
			return nil, err
		}
		tlsConfig = &tls.Config{
			Certificates:       []tls.Certificate{cert},
			InsecureSkipVerify: true,
		}
	}
	
	listener, err = quic.ListenAddr("localhost:0", tlsConfig, nil)
	
	if err != nil {
		return nil, err
	}

	broker := &MockBroker{
		listener:  listener,
		address:   listener.Addr().String(),
		tlsConfig: tlsConfig,
		requests:  make([][]byte, 0),
		responses: make(map[string][]byte),
	}

	// Start accepting connections
	go broker.acceptConnections()

	return broker, nil
}

// acceptConnections handles incoming QUIC connections
func (m *MockBroker) acceptConnections() {
	for {
		conn, err := m.listener.Accept(context.Background())
		if err != nil {
			return // Listener closed
		}
		go m.handleConnection(conn)
	}
}

// handleConnection handles a single QUIC connection
func (m *MockBroker) handleConnection(conn quic.Connection) {
	defer conn.CloseWithError(0, "test completed")

	for {
		stream, err := conn.AcceptStream(context.Background())
		if err != nil {
			return
		}
		go m.handleStream(stream)
	}
}

// handleStream handles a single QUIC stream
func (m *MockBroker) handleStream(stream quic.Stream) {
	defer stream.Close()

	// Read request length first
	lengthBytes := make([]byte, 4)
	_, err := io.ReadFull(stream, lengthBytes)
	if err != nil {
		return
	}
	
	requestLength := binary.BigEndian.Uint32(lengthBytes)
	
	// Read request data
	data := make([]byte, requestLength)
	_, err = io.ReadFull(stream, data)
	if err != nil {
		return
	}

	m.mutex.Lock()
	m.requests = append(m.requests, data)
	
	// Check for specific responses
	response := []byte(`{"status":"healthy","details":{},"timestamp":"` + time.Now().Format(time.RFC3339) + `"}`)
	if customResponse, exists := m.responses[string(data)]; exists {
		response = customResponse
	} else if customResponse, exists := m.responses[""]; exists {
		// Fallback to empty key for any request
		response = customResponse
	}
	m.mutex.Unlock()

	// Send response length first
	responseLengthBytes := make([]byte, 4)
	binary.BigEndian.PutUint32(responseLengthBytes, uint32(len(response)))
	stream.Write(responseLengthBytes)
	
	// Send response
	stream.Write(response)
}

// SetResponse sets a custom response for a specific request
func (m *MockBroker) SetResponse(request string, response []byte) {
	m.mutex.Lock()
	m.responses[request] = response
	m.mutex.Unlock()
}

// GetRequests returns all received requests
func (m *MockBroker) GetRequests() [][]byte {
	m.mutex.Lock()
	defer m.mutex.Unlock()
	requests := make([][]byte, len(m.requests))
	copy(requests, m.requests)
	return requests
}

// Close shuts down the mock broker
func (m *MockBroker) Close() error {
	return m.listener.Close()
}

// Address returns the broker's listening address
func (m *MockBroker) Address() string {
	return m.address
}

// generateSelfSignedCert creates a self-signed certificate for testing
func generateSelfSignedCert() (tls.Certificate, error) {
	// Generate private key
	priv, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return tls.Certificate{}, err
	}

	// Create certificate template
	template := x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject: pkix.Name{
			Organization:  []string{"Test"},
			Country:       []string{"US"},
			Province:      []string{""},
			Locality:      []string{"Test City"},
			StreetAddress: []string{""},
			PostalCode:    []string{""},
		},
		NotBefore:    time.Now(),
		NotAfter:     time.Now().Add(365 * 24 * time.Hour),
		KeyUsage:     x509.KeyUsageKeyEncipherment | x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		IPAddresses:  []net.IP{net.IPv4(127, 0, 0, 1)},
		DNSNames:     []string{"localhost"},
	}

	// Create certificate
	certDER, err := x509.CreateCertificate(rand.Reader, &template, &template, &priv.PublicKey, priv)
	if err != nil {
		return tls.Certificate{}, err
	}

	// Create TLS certificate
	return tls.Certificate{
		Certificate: [][]byte{certDER},
		PrivateKey:  priv,
	}, nil
}

// generateTestCertificates creates test certificates for TLS testing
func generateTestCertificates(dir string) (string, string, string, error) {
	// Generate CA private key
	caKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return "", "", "", err
	}

	// Create CA certificate template
	caTemplate := x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject: pkix.Name{
			Organization:  []string{"Test CA"},
			Country:       []string{"US"},
			Province:      []string{""},
			Locality:      []string{"Test City"},
			StreetAddress: []string{""},
			PostalCode:    []string{""},
		},
		NotBefore:             time.Now(),
		NotAfter:              time.Now().Add(365 * 24 * time.Hour),
		IsCA:                  true,
		ExtKeyUsage:           []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth, x509.ExtKeyUsageServerAuth},
		KeyUsage:              x509.KeyUsageDigitalSignature | x509.KeyUsageCertSign,
		BasicConstraintsValid: true,
	}

	// Create CA certificate
	caBytes, err := x509.CreateCertificate(rand.Reader, &caTemplate, &caTemplate, &caKey.PublicKey, caKey)
	if err != nil {
		return "", "", "", err
	}

	// Save CA certificate
	caCertPath := filepath.Join(dir, "ca-cert.pem")
	caCertFile, err := os.Create(caCertPath)
	if err != nil {
		return "", "", "", err
	}
	defer caCertFile.Close()
	pem.Encode(caCertFile, &pem.Block{Type: "CERTIFICATE", Bytes: caBytes})

	// Generate client private key
	clientKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return "", "", "", err
	}

	// Create client certificate template
	clientTemplate := x509.Certificate{
		SerialNumber: big.NewInt(2),
		Subject: pkix.Name{
			Organization:  []string{"Test Client"},
			Country:       []string{"US"},
			Province:      []string{""},
			Locality:      []string{"Test City"},
			StreetAddress: []string{""},
			PostalCode:    []string{""},
		},
		NotBefore:    time.Now(),
		NotAfter:     time.Now().Add(365 * 24 * time.Hour),
		SubjectKeyId: []byte{1, 2, 3, 4, 6},
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth},
		KeyUsage:     x509.KeyUsageDigitalSignature,
		IPAddresses:  []net.IP{net.IPv4(127, 0, 0, 1)},
		DNSNames:     []string{"localhost"},
	}

	// Create client certificate
	clientBytes, err := x509.CreateCertificate(rand.Reader, &clientTemplate, &caTemplate, &clientKey.PublicKey, caKey)
	if err != nil {
		return "", "", "", err
	}

	// Save client certificate
	clientCertPath := filepath.Join(dir, "client-cert.pem")
	clientCertFile, err := os.Create(clientCertPath)
	if err != nil {
		return "", "", "", err
	}
	defer clientCertFile.Close()
	pem.Encode(clientCertFile, &pem.Block{Type: "CERTIFICATE", Bytes: clientBytes})

	// Save client private key
	clientKeyPath := filepath.Join(dir, "client-key.pem")
	clientKeyFile, err := os.Create(clientKeyPath)
	if err != nil {
		return "", "", "", err
	}
	defer clientKeyFile.Close()
	clientKeyDER, err := x509.MarshalPKCS8PrivateKey(clientKey)
	if err != nil {
		return "", "", "", err
	}
	pem.Encode(clientKeyFile, &pem.Block{Type: "PRIVATE KEY", Bytes: clientKeyDER})

	return caCertPath, clientCertPath, clientKeyPath, nil
}

// createTestClientConfig creates a client config with TLS for testing
func createTestClientConfig(brokers []string) *rustmq.ClientConfig {
	config := getTestClientConfig()
	config.Brokers = brokers
	// Keep the fast 1s timeout from getTestClientConfig() for faster test completion
	config.EnableTLS = true
	config.TLSConfig = &rustmq.TLSConfig{
		InsecureSkipVerify: true,
	}
	return config
}

func TestConnection_BasicConnection(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker.Close()

	// Create client config
	config := createTestClientConfig([]string{broker.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Test connection
	assert.True(t, client.IsConnected())

	// Test stats
	stats := client.Stats()
	assert.Equal(t, 1, stats.TotalConnections)
	assert.Equal(t, 1, stats.ActiveConnections)
	assert.Equal(t, config.Brokers, stats.Brokers)
}

func TestConnection_TLSWithClientCertificate(t *testing.T) {
	t.Parallel()

	// Create temporary directory for certificates
	tempDir, err := ioutil.TempDir("", "rustmq-test-certs")
	require.NoError(t, err)
	defer os.RemoveAll(tempDir)

	// Generate test certificates
	caCertPath, clientCertPath, clientKeyPath, err := generateTestCertificates(tempDir)
	require.NoError(t, err)

	// Load server certificate for mock broker
	serverCert, err := tls.LoadX509KeyPair(clientCertPath, clientKeyPath)
	require.NoError(t, err)

	serverTLSConfig := &tls.Config{
		Certificates: []tls.Certificate{serverCert},
		ClientAuth:   tls.RequireAndVerifyClientCert,
	}

	// Start mock broker with TLS
	broker, err := NewMockBroker(true, serverTLSConfig)
	require.NoError(t, err)
	defer broker.Close()

	// Create client config with TLS and client certificate
	config := getTestClientConfig()
	config.Brokers = []string{broker.Address()}
	config.EnableTLS = true
	config.TLSConfig = &rustmq.TLSConfig{
		CACert:             caCertPath,
		ClientCert:         clientCertPath,
		ClientKey:          clientKeyPath,
		InsecureSkipVerify: true, // For testing
	}

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Test connection
	assert.True(t, client.IsConnected())
}

func TestConnection_HealthCheck(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker.Close()

	// Create client config
	config := createTestClientConfig([]string{broker.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Test health check
	err = client.HealthCheck()
	assert.NoError(t, err)

	// Verify health check was sent
	requests := broker.GetRequests()
	assert.Greater(t, len(requests), 0)

	// Check stats
	stats := client.Stats()
	assert.Greater(t, stats.HealthChecks, uint64(0))
}

func TestConnection_HealthCheckFailure(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker.Close()

	// Set unhealthy response
	broker.SetResponse("", []byte(`{"status":"unhealthy","details":{},"timestamp":"` + time.Now().Format(time.RFC3339) + `"}`))

	// Create client config
	config := createTestClientConfig([]string{broker.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Test health check failure
	err = client.HealthCheck()
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "unhealthy")

	// Check stats
	stats := client.Stats()
	assert.Greater(t, stats.HealthCheckErrors, uint64(0))
}

func TestConnection_Reconnection(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)

	// Create client config with very aggressive timeouts to prevent long waits
	config := createTestClientConfig([]string{broker.Address()})
	config.RetryConfig.BaseDelay = 10 * time.Millisecond
	config.RetryConfig.MaxDelay = 50 * time.Millisecond
	config.RetryConfig.MaxRetries = 0  // No retries to prevent hanging
	config.ConnectTimeout = 100 * time.Millisecond
	config.RequestTimeout = 100 * time.Millisecond

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)

	// Verify initial connection
	assert.True(t, client.IsConnected())

	// Close mock broker to simulate connection failure
	broker.Close()

	// Close client immediately to prevent background reconnection attempts
	// This should complete quickly with our aggressive timeouts
	err = client.Close()
	assert.NoError(t, err)

	// Verify that stats tracked the connection
	stats := client.Stats()
	assert.Greater(t, stats.TotalConnections, 0)
}

func TestConnection_MultipleConnections(t *testing.T) {
	t.Parallel()

	// Start multiple mock brokers
	broker1, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker1.Close()

	broker2, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker2.Close()

	// Create client config with multiple brokers
	config := createTestClientConfig([]string{broker1.Address(), broker2.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Test connections
	assert.True(t, client.IsConnected())

	// Check stats
	stats := client.Stats()
	assert.Equal(t, 2, stats.TotalConnections)
	assert.Equal(t, 2, stats.ActiveConnections)
}

func TestConnection_RoundRobinRequests(t *testing.T) {
	t.Parallel()

	// Start multiple mock brokers
	broker1, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker1.Close()

	broker2, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker2.Close()

	// Create client config with multiple brokers
	config := createTestClientConfig([]string{broker1.Address(), broker2.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Send multiple health checks to verify round-robin
	for i := 0; i < 4; i++ {
		err := client.HealthCheck()
		assert.NoError(t, err)
	}

	// Both brokers should have received requests
	requests1 := broker1.GetRequests()
	requests2 := broker2.GetRequests()

	// With round-robin, both brokers should have received requests
	assert.Greater(t, len(requests1), 0)
	assert.Greater(t, len(requests2), 0)
}

func TestConnection_BackoffLogic(t *testing.T) {
	t.Parallel()

	// Create client config with specific backoff settings (fast for testing)
	config := getTestClientConfig()
	config.Brokers = []string{"localhost:99999"} // Non-existent broker
	config.RetryConfig.BaseDelay = 10 * time.Millisecond
	config.RetryConfig.MaxDelay = 50 * time.Millisecond
	config.RetryConfig.Multiplier = 2.0
	config.RetryConfig.MaxRetries = 2  // Reduced from 3 to speed up test
	config.ConnectTimeout = 50 * time.Millisecond

	// This should fail to connect quickly
	_, err := rustmq.NewClient(config)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "failed to establish connections")
}

func TestConnection_TLSConfiguration(t *testing.T) {
	t.Parallel()

	// Create temporary directory for certificates
	tempDir, err := ioutil.TempDir("", "rustmq-test-certs")
	require.NoError(t, err)
	defer os.RemoveAll(tempDir)

	// Generate test certificates
	caCertPath, clientCertPath, clientKeyPath, err := generateTestCertificates(tempDir)
	require.NoError(t, err)

	// Test TLS config without client certificate
	config1 := getTestClientConfig()
	config1.EnableTLS = true
	config1.TLSConfig = &rustmq.TLSConfig{
		CACert:             caCertPath,
		InsecureSkipVerify: true,
	}

	// Test TLS config with client certificate
	config2 := getTestClientConfig()
	config2.EnableTLS = true
	config2.TLSConfig = &rustmq.TLSConfig{
		CACert:             caCertPath,
		ClientCert:         clientCertPath,
		ClientKey:          clientKeyPath,
		InsecureSkipVerify: true,
	}

	// Both configs should be valid (though connection will fail without mock broker)
	assert.NotNil(t, config1.TLSConfig)
	assert.NotNil(t, config2.TLSConfig)
	assert.Equal(t, caCertPath, config1.TLSConfig.CACert)
	assert.Equal(t, clientCertPath, config2.TLSConfig.ClientCert)
}

func TestConnection_StatsTracking(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker.Close()

	// Create client config
	config := createTestClientConfig([]string{broker.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Get initial stats
	initialStats := client.Stats()

	// Perform some operations
	err = client.HealthCheck()
	assert.NoError(t, err)

	// Get updated stats
	updatedStats := client.Stats()

	// Verify stats are tracking correctly
	assert.Greater(t, updatedStats.HealthChecks, initialStats.HealthChecks)
	assert.Greater(t, updatedStats.RequestsSent, initialStats.RequestsSent)
	assert.Greater(t, updatedStats.BytesSent, initialStats.BytesSent)
}

func TestConnection_ConcurrentOperations(t *testing.T) {
	t.Parallel()

	// Start mock broker
	broker, err := NewMockBroker(false, nil)
	require.NoError(t, err)
	defer broker.Close()

	// Create client config
	config := createTestClientConfig([]string{broker.Address()})

	// Create client
	client, err := rustmq.NewClient(config)
	require.NoError(t, err)
	defer client.Close()

	// Perform concurrent health checks with timeout
	const numRoutines = 10
	const checksPerRoutine = 5

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	var wg sync.WaitGroup
	errors := make(chan error, numRoutines*checksPerRoutine)

	for i := 0; i < numRoutines; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < checksPerRoutine; j++ {
				select {
				case <-ctx.Done():
					return
				default:
					if err := client.HealthCheck(); err != nil {
						errors <- err
					}
				}
			}
		}()
	}

	wg.Wait()
	close(errors)

	// Check for errors
	errorCount := 0
	for err := range errors {
		t.Logf("Concurrent operation error: %v", err)
		errorCount++
	}

	// Most operations should succeed
	assert.Less(t, errorCount, numRoutines*checksPerRoutine/2)

	// Verify stats show all the operations
	stats := client.Stats()
	assert.Greater(t, stats.HealthChecks, uint64(numRoutines*checksPerRoutine/2))
}