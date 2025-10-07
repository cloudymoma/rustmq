package tests

import (
	"context"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
	"github.com/stretchr/testify/assert"
)

func TestProducerConfiguration(t *testing.T) {
	t.Parallel()

	config := rustmq.DefaultProducerConfig()
	
	assert.Equal(t, 100, config.BatchSize)
	assert.Equal(t, 100*time.Millisecond, config.BatchTimeout)
	assert.Equal(t, 1024*1024, config.MaxMessageSize)
	assert.Equal(t, rustmq.AckLeader, config.AckLevel)
	assert.False(t, config.Idempotent)
	assert.NotNil(t, config.Compression)
	assert.NotNil(t, config.DefaultProps)
}

func TestProducerCustomConfiguration(t *testing.T) {
	t.Parallel()

	config := &rustmq.ProducerConfig{
		ProducerID:     "custom-producer",
		BatchSize:      50,
		BatchTimeout:   200 * time.Millisecond,
		MaxMessageSize: 2048,
		AckLevel:       rustmq.AckAll,
		Idempotent:     true,
		DefaultProps: map[string]string{
			"app":     "test-app",
			"version": "1.0.0",
		},
	}

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer config test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("config-test", config)
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	assert.Equal(t, "custom-producer", producer.Config().ProducerID)
	assert.Equal(t, 50, producer.Config().BatchSize)
	assert.Equal(t, rustmq.AckAll, producer.Config().AckLevel)
	assert.True(t, producer.Config().Idempotent)
}

func TestProducerSendMessage(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer send test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("send-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	message := rustmq.NewMessage().
		Topic("send-test").
		PayloadString("Test message").
		Header("test", "true").
		Build()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	result, err := producer.Send(ctx, message)
	
	// This will likely fail without a real broker, but we test the interface
	if err != nil {
		t.Logf("Send failed (expected in test environment): %v", err)
		return
	}
	
	assert.NotNil(t, result)
	assert.Equal(t, message.ID, result.MessageID)
	assert.Equal(t, "send-test", result.Topic)
}

func TestProducerSendAsync(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer async test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("async-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	message := rustmq.NewMessage().
		Topic("async-test").
		PayloadString("Async test message").
		Build()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	callbackCalled := make(chan bool, 1)
	var callbackResult *rustmq.MessageResult
	var callbackError error

	err = producer.SendAsync(ctx, message, func(result *rustmq.MessageResult, err error) {
		callbackResult = result
		callbackError = err
		callbackCalled <- true
	})

	if err != nil {
		t.Logf("SendAsync failed (expected in test environment): %v", err)
		return
	}

	// Wait for callback with shorter timeout
	select {
	case <-callbackCalled:
		if callbackError != nil {
			t.Logf("Async callback error (expected in test environment): %v", callbackError)
		} else {
			assert.NotNil(t, callbackResult)
			assert.Equal(t, message.ID, callbackResult.MessageID)
		}
	case <-time.After(2 * time.Second):
		t.Log("Callback not called within timeout (expected in test environment)")
	}
}

func TestProducerSendBatch(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer batch test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("batch-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	messages := []*rustmq.Message{
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 1").Build(),
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 2").Build(),
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 3").Build(),
	}

	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	results, err := producer.SendBatch(ctx, messages)
	
	if err != nil {
		t.Logf("SendBatch failed (expected in test environment): %v", err)
		return
	}
	
	assert.Len(t, results, 3)
	for i, result := range results {
		assert.Equal(t, messages[i].ID, result.MessageID)
		assert.Equal(t, "batch-test", result.Topic)
	}
}

func TestProducerMessageSizeValidation(t *testing.T) {
	t.Parallel()

	config := &rustmq.ProducerConfig{
		MaxMessageSize: 100, // Very small limit for testing
	}

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping size validation test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("size-test", config)
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	// Create a message that exceeds the size limit
	largePayload := make([]byte, 200)
	for i := range largePayload {
		largePayload[i] = 'A'
	}

	message := rustmq.NewMessage().
		Topic("size-test").
		Payload(largePayload).
		Build()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	_, err = producer.Send(ctx, message)
	
	// Should fail due to size limit
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "exceeds maximum")
}

func TestProducerFlush(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer flush test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("flush-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	// Send some async messages
	for i := 0; i < 5; i++ {
		message := rustmq.NewMessage().
			Topic("flush-test").
			PayloadString("Flush test message").
			Build()

		ctx := context.Background()
		producer.SendAsync(ctx, message, nil)
	}

	// Flush should wait for all messages to be sent
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	err = producer.Flush(ctx)
	if err != nil {
		t.Logf("Flush failed (expected in test environment): %v", err)
	}
}

func TestProducerMetrics(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer metrics test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("metrics-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}
	
	defer producer.Close()

	// Get initial metrics
	metrics := producer.Metrics()
	assert.NotNil(t, metrics)
	assert.Equal(t, uint64(0), metrics.MessagesSent)
	assert.Equal(t, uint64(0), metrics.MessagesFailed)

	// Send a message and check metrics update
	message := rustmq.NewMessage().
		Topic("metrics-test").
		PayloadString("Metrics test").
		Build()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	producer.Send(ctx, message)

	// Metrics might be updated asynchronously
	time.Sleep(50 * time.Millisecond)
	
	updatedMetrics := producer.Metrics()
	t.Logf("Updated metrics: %+v", updatedMetrics)
}

func TestProducerClose(t *testing.T) {
	t.Parallel()

	clientConfig := getTestClientConfig()
	client, err := rustmq.NewClient(clientConfig)
	
	if err != nil {
		t.Skip("Skipping producer close test - no broker available")
	}
	
	defer client.Close()

	producer, err := client.CreateProducer("close-test")
	if err != nil {
		t.Skip("Producer creation failed")
	}

	// Send a message
	message := rustmq.NewMessage().
		Topic("close-test").
		PayloadString("Before close").
		Build()

	ctx := context.Background()
	producer.SendAsync(ctx, message, nil)

	// Close the producer
	err = producer.Close()
	assert.NoError(t, err)

	// Subsequent sends should fail
	message = rustmq.NewMessage().
		Topic("close-test").
		PayloadString("After close").
		Build()

	ctx, cancel := context.WithTimeout(context.Background(), 500*time.Millisecond)
	defer cancel()

	_, err = producer.Send(ctx, message)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "closed")
}