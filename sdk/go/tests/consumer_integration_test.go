package tests

import (
	"encoding/json"
	"fmt"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
	"github.com/stretchr/testify/assert"
)

// Mock connection for testing without real broker
type MockConnection struct {
	responses map[string]interface{}
	requests  [][]byte
}

func (mc *MockConnection) sendRequest(request []byte) ([]byte, error) {
	mc.requests = append(mc.requests, request)
	
	// Parse request to determine response
	var reqMap map[string]interface{}
	if err := json.Unmarshal(request, &reqMap); err != nil {
		return nil, err
	}
	
	reqType, ok := reqMap["type"].(string)
	if !ok {
		return nil, fmt.Errorf("invalid request type")
	}
	
	// Return appropriate mock response
	switch reqType {
	case "join_consumer_group":
		response := rustmq.ConsumerGroupJoinResponse{
			Success:      true,
			MemberID:     "test-member-1",
			GenerationID: 1,
			Assignment: []rustmq.TopicPartition{
				{Topic: "test-topic", Partition: 0},
				{Topic: "test-topic", Partition: 1},
			},
		}
		return json.Marshal(response)
		
	case "fetch_messages":
		// Mock some messages
		messages := []*rustmq.Message{
			{
				ID:        "msg-1",
				Topic:     "test-topic",
				Partition: 0,
				Offset:    100,
				Payload:   []byte("test payload 1"),
				Timestamp: time.Now(),
				Headers:   map[string]string{"test": "header1"},
			},
			{
				ID:        "msg-2", 
				Topic:     "test-topic",
				Partition: 0,
				Offset:    101,
				Payload:   []byte("test payload 2"),
				Timestamp: time.Now(),
				Headers:   map[string]string{"test": "header2"},
			},
		}
		
		response := rustmq.FetchResponse{
			Success:     true,
			Messages:    messages,
			EndOffset:   102,
			PartitionID: 0,
			Watermarks: &rustmq.Watermarks{
				Low:  90,
				High: 102,
			},
		}
		return json.Marshal(response)
		
	case "commit_offset":
		response := rustmq.CommitResponse{
			Success: true,
			Results: make(map[string]*string),
		}
		return json.Marshal(response)
		
	case "seek_partition":
		response := rustmq.SeekResponse{
			Success:   true,
			NewOffset: 150,
		}
		return json.Marshal(response)
		
	case "health_check":
		response := rustmq.HealthCheck{
			Status:    rustmq.HealthStatusHealthy,
			Details:   map[string]string{"status": "ok"},
			Timestamp: time.Now(),
		}
		return json.Marshal(response)
		
	default:
		return nil, fmt.Errorf("unsupported request type: %s", reqType)
	}
}

func TestConsumerProtocolImplementation(t *testing.T) {
	// Test protocol structure serialization
	t.Run("FetchRequest", func(t *testing.T) {
		offset := uint64(100)
		req := rustmq.FetchRequest{
			Type:          "fetch_messages",
			Topic:         "test-topic",
			ConsumerGroup: "test-group",
			ConsumerID:    "consumer-1",
			MaxMessages:   50,
			MaxBytes:      1024 * 1024,
			TimeoutMs:     5000,
			FromOffset:    &offset,
			Timestamp:     time.Now(),
		}
		
		data, err := json.Marshal(req)
		assert.NoError(t, err)
		assert.Contains(t, string(data), "fetch_messages")
		assert.Contains(t, string(data), "test-topic")
		
		// Test deserialization
		var parsed rustmq.FetchRequest
		err = json.Unmarshal(data, &parsed)
		assert.NoError(t, err)
		assert.Equal(t, req.Type, parsed.Type)
		assert.Equal(t, req.Topic, parsed.Topic)
		assert.Equal(t, *req.FromOffset, *parsed.FromOffset)
	})
	
	t.Run("CommitRequest", func(t *testing.T) {
		req := rustmq.CommitRequest{
			Type:          "commit_offset",
			ConsumerGroup: "test-group",
			ConsumerID:    "consumer-1",
			Offsets: map[string]rustmq.OffsetAndMetadata{
				"test-topic:0": rustmq.NewOffsetAndMetadata(100, nil),
			},
			Timestamp: time.Now(),
		}
		
		data, err := json.Marshal(req)
		assert.NoError(t, err)
		assert.Contains(t, string(data), "commit_offset")
		
		var parsed rustmq.CommitRequest
		err = json.Unmarshal(data, &parsed)
		assert.NoError(t, err)
		assert.Equal(t, req.Type, parsed.Type)
		assert.Equal(t, uint64(100), parsed.Offsets["test-topic:0"].Offset)
	})
	
	t.Run("SeekRequest", func(t *testing.T) {
		offset := uint64(150)
		timestamp := time.Now()
		
		// Test offset seek
		offsetReq := rustmq.SeekRequest{
			Type:          "seek_partition",
			Topic:         "test-topic", 
			ConsumerGroup: "test-group",
			ConsumerID:    "consumer-1",
			Partition:     0,
			Offset:        &offset,
		}
		
		data, err := json.Marshal(offsetReq)
		assert.NoError(t, err)
		assert.Contains(t, string(data), "seek_partition")
		
		// Test timestamp seek
		timestampReq := rustmq.SeekRequest{
			Type:          "seek_partition",
			Topic:         "test-topic",
			ConsumerGroup: "test-group", 
			ConsumerID:    "consumer-1",
			Partition:     0,
			Timestamp:     &timestamp,
		}
		
		data, err = json.Marshal(timestampReq)
		assert.NoError(t, err)
		assert.Contains(t, string(data), "seek_partition")
	})
}

func TestRetryQueueStructure(t *testing.T) {
	// Test basic retry queue structure concepts
	failedMessages := make(map[string]int) // message_key -> retry_count
	maxRetries := 3
	
	// Simulate failed message
	key := "test-topic:100"
	failedMessages[key] = 1
	
	// Check retry count
	retryCount, exists := failedMessages[key]
	assert.True(t, exists)
	assert.Equal(t, 1, retryCount)
	
	// Increment retry count
	failedMessages[key] = retryCount + 1
	assert.Equal(t, 2, failedMessages[key])
	
	// Check max retries logic
	if failedMessages[key] >= maxRetries {
		delete(failedMessages, key) // Would send to DLQ
	}
	
	// Should still exist since 2 < 3
	_, exists = failedMessages[key]
	assert.True(t, exists)
	
	// Exceed max retries
	failedMessages[key] = maxRetries
	if failedMessages[key] >= maxRetries {
		delete(failedMessages, key) // Send to DLQ
	}
	
	_, exists = failedMessages[key]
	assert.False(t, exists)
}

func TestConsumerMetricsCalculation(t *testing.T) {
	// Test lag calculation concepts
	committedOffset := uint64(100)
	highWaterMark := uint64(150)
	
	// Calculate lag
	var lag uint64
	if highWaterMark > committedOffset {
		lag = highWaterMark - committedOffset
	}
	
	assert.Equal(t, uint64(50), lag)
	
	// Test when no lag
	committedOffset = 150
	if highWaterMark > committedOffset {
		lag = highWaterMark - committedOffset
	} else {
		lag = 0
	}
	
	assert.Equal(t, uint64(0), lag)
	
	// Test metrics structure
	metrics := &rustmq.ConsumerMetrics{
		MessagesReceived:  100,
		MessagesProcessed: 95,
		MessagesFailed:    5,
		BytesReceived:     1024000,
		Lag:               lag,
		LastReceiveTime:   time.Now(),
		ProcessingTimeMs:  15.5,
	}
	
	assert.Equal(t, uint64(100), metrics.MessagesReceived)
	assert.Equal(t, uint64(95), metrics.MessagesProcessed)
	assert.Equal(t, uint64(5), metrics.MessagesFailed)
	assert.Equal(t, uint64(0), metrics.Lag)
}

func TestOffsetTracking(t *testing.T) {
	// Test offset tracking logic
	committedOffset := uint64(100)
	pendingOffsets := make(map[uint64]bool)
	
	// Add pending offsets
	pendingOffsets[101] = true
	pendingOffsets[102] = true
	pendingOffsets[104] = true // Gap at 103
	
	// Simulate acknowledging offset 101
	delete(pendingOffsets, 101)
	
	// Update committed offset to highest consecutive
	for {
		nextOffset := committedOffset + 1
		if _, exists := pendingOffsets[nextOffset]; !exists {
			committedOffset = nextOffset
			delete(pendingOffsets, nextOffset)
		} else {
			break
		}
	}
	
	// Should advance to 101 but not 102 due to gap
	assert.Equal(t, uint64(101), committedOffset)
	assert.True(t, pendingOffsets[102])
	assert.True(t, pendingOffsets[104])
	
	// Acknowledge 102
	delete(pendingOffsets, 102)
	for {
		nextOffset := committedOffset + 1
		if _, exists := pendingOffsets[nextOffset]; !exists {
			committedOffset = nextOffset
			delete(pendingOffsets, nextOffset)
		} else {
			break
		}
	}
	
	// Should advance to 102, then check 103 (not pending), advance to 103, then hit gap before 104
	assert.Equal(t, uint64(103), committedOffset)
	assert.True(t, pendingOffsets[104])
}

func TestConsumerMessageInterface(t *testing.T) {
	message := &rustmq.Message{
		ID:        "test-msg-1",
		Topic:     "test-topic",
		Partition: 0,
		Offset:    100,
		Payload:   []byte("test payload"),
		Headers:   map[string]string{"key": "value"},
		Timestamp: time.Now(),
	}
	
	ackChan := make(chan rustmq.AckRequest, 2)
	receivedTime := time.Now()
	
	// Test ConsumerMessage structure
	consumerMessage := struct {
		Message      *rustmq.Message
		ackChan      chan rustmq.AckRequest
		partition    uint32
		retryCount   int
		receivedTime time.Time
	}{
		Message:      message,
		ackChan:      ackChan,
		partition:    0,
		retryCount:   0,
		receivedTime: receivedTime,
	}
	
	// Test basic properties
	assert.Equal(t, uint32(0), consumerMessage.partition)
	assert.Equal(t, 0, consumerMessage.retryCount)
	assert.True(t, time.Since(consumerMessage.receivedTime) >= 0)
	
	// Test acknowledgment request structure
	ackReq := rustmq.AckRequest{
		Offset:  message.Offset,
		Success: true,
	}
	
	// Send ack request
	select {
	case ackChan <- ackReq:
		// Success
	default:
		t.Fatal("Failed to send ack request")
	}
	
	// Verify ack request was sent
	select {
	case receivedAck := <-ackChan:
		assert.Equal(t, uint64(100), receivedAck.Offset)
		assert.True(t, receivedAck.Success)
	case <-time.After(time.Second):
		t.Fatal("Ack request not received")
	}
	
	// Test negative acknowledgment
	nackReq := rustmq.AckRequest{
		Offset:  message.Offset,
		Success: false,
	}
	
	select {
	case ackChan <- nackReq:
		// Success
	default:
		t.Fatal("Failed to send nack request")
	}
	
	select {
	case receivedNack := <-ackChan:
		assert.Equal(t, uint64(100), receivedNack.Offset)
		assert.False(t, receivedNack.Success)
	case <-time.After(time.Second):
		t.Fatal("Nack request not received")
	}
}

func TestStartPositionTypes(t *testing.T) {
	testCases := []struct {
		name     string
		position rustmq.StartPosition
	}{
		{
			name: "Earliest",
			position: rustmq.StartPosition{
				Type: rustmq.StartEarliest,
			},
		},
		{
			name: "Latest", 
			position: rustmq.StartPosition{
				Type: rustmq.StartLatest,
			},
		},
		{
			name: "Specific Offset",
			position: rustmq.StartPosition{
				Type:   rustmq.StartOffset,
				Offset: &[]uint64{12345}[0],
			},
		},
		{
			name: "Specific Timestamp",
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
				assert.NotNil(t, config.StartPosition.Offset)
				assert.Equal(t, *tc.position.Offset, *config.StartPosition.Offset)
			}
			
			if tc.position.Timestamp != nil {
				assert.NotNil(t, config.StartPosition.Timestamp)
				assert.Equal(t, tc.position.Timestamp.Unix(), config.StartPosition.Timestamp.Unix())
			}
		})
	}
}

func TestDeadLetterQueueConfiguration(t *testing.T) {
	config := &rustmq.ConsumerConfig{
		ConsumerGroup:      "test-group",
		MaxRetryAttempts:   3,
		DeadLetterQueue:    "test-dlq",
	}
	
	assert.Equal(t, "test-group", config.ConsumerGroup)
	assert.Equal(t, 3, config.MaxRetryAttempts)
	assert.Equal(t, "test-dlq", config.DeadLetterQueue)
}

func BenchmarkOffsetTracking(b *testing.B) {
	// Benchmark offset tracking logic with local variables
	committedOffset := uint64(0)
	pendingOffsets := make(map[uint64]bool)

	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		offset := uint64(i + 1)
		pendingOffsets[offset] = true

		// Simulate acknowledgment
		delete(pendingOffsets, offset)

		// Update committed offset if this is the next sequential offset
		if offset == committedOffset+1 {
			committedOffset = offset
		}
	}
}

func BenchmarkMessageSerialization(b *testing.B) {
	req := rustmq.FetchRequest{
		Type:          "fetch_messages",
		Topic:         "benchmark-topic",
		ConsumerGroup: "benchmark-group",
		ConsumerID:    "benchmark-consumer",
		MaxMessages:   100,
		MaxBytes:      1024 * 1024,
		TimeoutMs:     5000,
		Timestamp:     time.Now(),
	}
	
	b.ResetTimer()
	
	for i := 0; i < b.N; i++ {
		_, err := json.Marshal(req)
		if err != nil {
			b.Fatal(err)
		}
	}
}