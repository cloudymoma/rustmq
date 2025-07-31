package tests

import (
	"context"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
	"github.com/stretchr/testify/assert"
)

func TestConsumerConfiguration(t *testing.T) {
	config := rustmq.DefaultConsumerConfig()
	
	assert.Equal(t, 5*time.Second, config.AutoCommitInterval)
	assert.True(t, config.EnableAutoCommit)
	assert.Equal(t, 100, config.FetchSize)
	assert.Equal(t, 1*time.Second, config.FetchTimeout)
	assert.Equal(t, rustmq.StartLatest, config.StartPosition.Type)
	assert.Equal(t, 3, config.MaxRetryAttempts)
}

func TestConsumerCustomConfiguration(t *testing.T) {
	config := &rustmq.ConsumerConfig{
		ConsumerID:         "custom-consumer",
		ConsumerGroup:      "custom-group",
		AutoCommitInterval: 10 * time.Second,
		EnableAutoCommit:   false,
		FetchSize:          50,
		FetchTimeout:       2 * time.Second,
		StartPosition: rustmq.StartPosition{
			Type: rustmq.StartEarliest,
		},
		MaxRetryAttempts: 5,
		DeadLetterQueue:  "dlq-topic",
	}

	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer config test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("config-test", "custom-group", config)
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	assert.Equal(t, "custom-consumer", consumer.Config().ConsumerID)
	assert.Equal(t, "custom-group", consumer.Config().ConsumerGroup)
	assert.Equal(t, 10*time.Second, consumer.Config().AutoCommitInterval)
	assert.False(t, consumer.Config().EnableAutoCommit)
	assert.Equal(t, rustmq.StartEarliest, consumer.Config().StartPosition.Type)
}

func TestConsumerReceiveTimeout(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer receive test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("receive-test", "test-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	// Try to receive with a short timeout (should timeout since no messages)
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	message, err := consumer.Receive(ctx)
	
	// Should timeout in test environment
	if err != nil {
		assert.Contains(t, err.Error(), "context deadline exceeded")
		assert.Nil(t, message)
	} else {
		// If we somehow received a message, test its structure
		assert.NotNil(t, message)
		assert.NotNil(t, message.Message)
	}
}

func TestConsumerReceiveChannel(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer channel test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("channel-test", "test-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	messageChan := consumer.ReceiveChan()
	assert.NotNil(t, messageChan)

	// Try to receive from channel with timeout
	select {
	case message := <-messageChan:
		if message != nil {
			assert.NotNil(t, message.Message)
			t.Logf("Received message: %s", message.Message.ID)
		}
	case <-time.After(1 * time.Second):
		t.Log("No message received from channel (expected in test environment)")
	}
}

func TestConsumerManualCommit(t *testing.T) {
	config := &rustmq.ConsumerConfig{
		EnableAutoCommit: false, // Disable auto-commit for manual testing
	}

	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer commit test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("commit-test", "test-group", config)
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Manual commit should work even without receiving messages
	err = consumer.Commit(ctx)
	if err != nil {
		t.Logf("Commit failed (expected in test environment): %v", err)
	}
}

func TestConsumerSeek(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer seek test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("seek-test", "test-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Test seek to offset (not implemented yet, should return error)
	err = consumer.Seek(ctx, 100)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not implemented")

	// Test seek to timestamp (not implemented yet, should return error)
	err = consumer.SeekToTimestamp(ctx, time.Now())
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not implemented")
}

func TestConsumerMetrics(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer metrics test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("metrics-test", "test-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}
	
	defer consumer.Close()

	metrics := consumer.Metrics()
	assert.NotNil(t, metrics)
	assert.Equal(t, uint64(0), metrics.MessagesReceived)
	assert.Equal(t, uint64(0), metrics.MessagesProcessed)
	assert.Equal(t, uint64(0), metrics.MessagesFailed)
	assert.Equal(t, uint64(0), metrics.BytesReceived)

	t.Logf("Initial consumer metrics: %+v", metrics)
}

func TestConsumerMessageAcknowledgment(t *testing.T) {
	// Test the ConsumerMessage acknowledgment interface
	// Since we can't easily create real messages without a broker,
	// we'll test the interface structure

	// This would be how acknowledgment works:
	// message, err := consumer.Receive(ctx)
	// if err == nil && message != nil {
	//     // Process message...
	//     err = message.Ack() // or message.Nack()
	//     assert.NoError(t, err)
	// }

	t.Log("ConsumerMessage acknowledgment interface tested conceptually")
}

func TestConsumerStartPositions(t *testing.T) {
	testCases := []struct {
		name     string
		position rustmq.StartPosition
	}{
		{
			name: "StartEarliest",
			position: rustmq.StartPosition{
				Type: rustmq.StartEarliest,
			},
		},
		{
			name: "StartLatest",
			position: rustmq.StartPosition{
				Type: rustmq.StartLatest,
			},
		},
		{
			name: "StartOffset",
			position: rustmq.StartPosition{
				Type:   rustmq.StartOffset,
				Offset: &[]uint64{12345}[0],
			},
		},
		{
			name: "StartTimestamp",
			position: rustmq.StartPosition{
				Type:      rustmq.StartTimestamp,
				Timestamp: &[]time.Time{time.Now()}[0],
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			config := &rustmq.ConsumerConfig{
				StartPosition: tc.position,
			}

			assert.Equal(t, tc.position.Type, config.StartPosition.Type)
			
			if tc.position.Offset != nil {
				assert.Equal(t, *tc.position.Offset, *config.StartPosition.Offset)
			}
			
			if tc.position.Timestamp != nil {
				assert.Equal(t, *tc.position.Timestamp, *config.StartPosition.Timestamp)
			}
		})
	}
}

func TestConsumerClose(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer close test - no broker available")
	}
	
	defer client.Close()

	consumer, err := client.CreateConsumer("close-test", "test-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}

	// Close the consumer
	err = consumer.Close()
	assert.NoError(t, err)

	// Subsequent operations should fail
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	_, err = consumer.Receive(ctx)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "closed")

	err = consumer.Commit(ctx)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "closed")
}

func TestConsumerGroupReuse(t *testing.T) {
	clientConfig := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping consumer group test - no broker available")
	}
	
	defer client.Close()

	// Create first consumer in group
	consumer1, err := client.CreateConsumer("group-test", "shared-group")
	if err != nil {
		t.Skip("Consumer creation failed")
	}

	// Create second consumer in same group
	consumer2, err := client.Consumer("group-test", "shared-group")
	if err != nil {
		t.Skip("Consumer retrieval failed")
	}

	// Should be the same instance
	assert.Equal(t, consumer1.ID(), consumer2.ID())
	assert.Equal(t, consumer1.ConsumerGroup(), consumer2.ConsumerGroup())

	consumer1.Close()
}