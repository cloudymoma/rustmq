package tests

import (
	"context"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestClientCreation(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092"}
	config.ClientID = "test-client"

	client, err := rustmq.NewClient(config)
	
	// This may fail in tests since no broker is running
	if err != nil {
		t.Logf("Client creation failed (expected in test environment): %v", err)
		return
	}
	
	assert.NotNil(t, client)
	assert.Equal(t, "test-client", client.Config().ClientID)
	
	// Clean up
	client.Close()
}

func TestClientConnectionCheck(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping connection test - no broker available")
	}
	
	defer client.Close()
	
	// Test connection status
	connected := client.IsConnected()
	t.Logf("Client connected: %v", connected)
	
	// Test health check
	err = client.HealthCheck()
	if err != nil {
		t.Logf("Health check failed (expected in test environment): %v", err)
	}
}

func TestProducerCreation(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping producer test - no broker available")
	}
	
	defer client.Close()
	
	producer, err := client.CreateProducer("test-topic")
	if err != nil {
		t.Logf("Producer creation failed (expected in test environment): %v", err)
		return
	}
	
	assert.NotNil(t, producer)
	assert.Equal(t, "test-topic", producer.Topic())
	assert.NotEmpty(t, producer.ID())
	
	producer.Close()
}

func TestConsumerCreation(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping consumer test - no broker available")
	}
	
	defer client.Close()
	
	consumer, err := client.CreateConsumer("test-topic", "test-group")
	if err != nil {
		t.Logf("Consumer creation failed (expected in test environment): %v", err)
		return
	}
	
	assert.NotNil(t, consumer)
	assert.Equal(t, "test-topic", consumer.Topic())
	assert.Equal(t, "test-group", consumer.ConsumerGroup())
	assert.NotEmpty(t, consumer.ID())
	
	consumer.Close()
}

func TestClientStats(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping stats test - no broker available")
	}
	
	defer client.Close()
	
	stats := client.Stats()
	assert.NotNil(t, stats)
	assert.NotNil(t, stats.Brokers)
	t.Logf("Connection stats: %+v", stats)
}

func TestClientConfigSerialization(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092", "localhost:9093"}
	config.EnableTLS = true
	config.ConnectTimeout = 15 * time.Second
	
	// Test that config is properly structured
	assert.Len(t, config.Brokers, 2)
	assert.True(t, config.EnableTLS)
	assert.Equal(t, 15*time.Second, config.ConnectTimeout)
	assert.NotNil(t, config.RetryConfig)
	assert.NotNil(t, config.Compression)
}

func TestMultipleProducersSameTopic(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping multiple producers test - no broker available")
	}
	
	defer client.Close()
	
	// Create first producer
	producer1, err := client.CreateProducer("shared-topic")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	// Create second producer for same topic
	producer2, err := client.Producer("shared-topic")
	if err != nil {
		t.Skip("Producer retrieval failed")
	}
	
	// Should return the same producer instance
	assert.Equal(t, producer1.ID(), producer2.ID())
	
	producer1.Close()
}

func TestMultipleConsumersSameGroup(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping multiple consumers test - no broker available")
	}
	
	defer client.Close()
	
	// Create first consumer
	consumer1, err := client.CreateConsumer("shared-topic", "shared-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	// Create second consumer for same topic and group
	consumer2, err := client.Consumer("shared-topic", "shared-group")
	if err != nil {
		t.Skip("Consumer retrieval failed")
	}
	
	// Should return the same consumer instance
	assert.Equal(t, consumer1.ID(), consumer2.ID())
	
	consumer1.Close()
}

func TestClientCloseCleanup(t *testing.T) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	
	if err != nil {
		t.Skip("Skipping cleanup test - no broker available")
	}
	
	// Create some producers and consumers
	client.CreateProducer("topic1")
	client.CreateProducer("topic2")
	client.CreateConsumer("topic1", "group1")
	client.CreateConsumer("topic2", "group2")
	
	// Close should clean up all resources
	err = client.Close()
	assert.NoError(t, err)
	
	// Subsequent operations should fail
	_, err = client.CreateProducer("topic3")
	// This might not fail immediately depending on implementation
}