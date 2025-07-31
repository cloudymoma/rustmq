package rustmq

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/uuid"
)

// Consumer receives messages from RustMQ topics
type Consumer struct {
	id            string
	topic         string
	consumerGroup string
	config        *ConsumerConfig
	client        *Client
	metrics       *ConsumerMetrics
	offsetTracker *OffsetTracker
	messageChan   chan *ConsumerMessage
	closed        int32
	mutex         sync.RWMutex
}

// ConsumerConfig holds consumer configuration
type ConsumerConfig struct {
	ConsumerID         string            `json:"consumer_id,omitempty"`
	ConsumerGroup      string            `json:"consumer_group"`
	AutoCommitInterval time.Duration     `json:"auto_commit_interval"`
	EnableAutoCommit   bool              `json:"enable_auto_commit"`
	FetchSize          int               `json:"fetch_size"`
	FetchTimeout       time.Duration     `json:"fetch_timeout"`
	StartPosition      StartPosition     `json:"start_position"`
	MaxRetryAttempts   int               `json:"max_retry_attempts"`
	DeadLetterQueue    string            `json:"dead_letter_queue,omitempty"`
}

// StartPosition represents where to start consuming messages
type StartPosition struct {
	Type      StartPositionType `json:"type"`
	Offset    *uint64           `json:"offset,omitempty"`
	Timestamp *time.Time        `json:"timestamp,omitempty"`
}

// StartPositionType represents different starting position types
type StartPositionType string

const (
	StartEarliest  StartPositionType = "earliest"
	StartLatest    StartPositionType = "latest"
	StartOffset    StartPositionType = "offset"
	StartTimestamp StartPositionType = "timestamp"
)

// ConsumerMessage represents a message received by the consumer
type ConsumerMessage struct {
	Message      *Message
	ackChan      chan AckRequest
	partition    uint32
	retryCount   int
	receivedTime time.Time
}

// AckRequest represents an acknowledgment request
type AckRequest struct {
	Offset  uint64
	Success bool
}

// ConsumerMetrics holds consumer performance metrics
type ConsumerMetrics struct {
	MessagesReceived  uint64        `json:"messages_received"`
	MessagesProcessed uint64        `json:"messages_processed"`
	MessagesFailed    uint64        `json:"messages_failed"`
	BytesReceived     uint64        `json:"bytes_received"`
	Lag               uint64        `json:"lag"`
	LastReceiveTime   time.Time     `json:"last_receive_time"`
	ProcessingTimeMs  float64       `json:"processing_time_ms"`
}

// OffsetTracker tracks message offsets for acknowledgment
type OffsetTracker struct {
	committedOffset uint64
	pendingOffsets  map[uint64]bool
	lastCommitTime  time.Time
	mutex           sync.RWMutex
}

// DefaultConsumerConfig returns default consumer configuration
func DefaultConsumerConfig() *ConsumerConfig {
	return &ConsumerConfig{
		AutoCommitInterval: 5 * time.Second,
		EnableAutoCommit:   true,
		FetchSize:          100,
		FetchTimeout:       1 * time.Second,
		StartPosition: StartPosition{
			Type: StartLatest,
		},
		MaxRetryAttempts: 3,
	}
}

// newConsumer creates a new consumer
func newConsumer(client *Client, topic, consumerGroup string, config *ConsumerConfig) (*Consumer, error) {
	if config.ConsumerID == "" {
		config.ConsumerID = fmt.Sprintf("consumer-%s", uuid.New().String()[:8])
	}

	consumer := &Consumer{
		id:            config.ConsumerID,
		topic:         topic,
		consumerGroup: consumerGroup,
		config:        config,
		client:        client,
		metrics:       &ConsumerMetrics{},
		offsetTracker: &OffsetTracker{
			pendingOffsets: make(map[uint64]bool),
		},
		messageChan: make(chan *ConsumerMessage, config.FetchSize),
	}

	// Start background consumption
	go consumer.startConsumption(client.Context())

	// Start auto-commit if enabled
	if config.EnableAutoCommit {
		go consumer.startAutoCommit(client.Context())
	}

	return consumer, nil
}

// Receive receives the next message
func (c *Consumer) Receive(ctx context.Context) (*ConsumerMessage, error) {
	if atomic.LoadInt32(&c.closed) == 1 {
		return nil, fmt.Errorf("consumer is closed")
	}

	select {
	case message := <-c.messageChan:
		if message == nil {
			return nil, fmt.Errorf("consumer is closed")
		}
		
		atomic.AddUint64(&c.metrics.MessagesReceived, 1)
		atomic.AddUint64(&c.metrics.BytesReceived, uint64(len(message.Message.Payload)))
		c.metrics.LastReceiveTime = time.Now()
		
		return message, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

// ReceiveChan returns a channel for receiving messages
func (c *Consumer) ReceiveChan() <-chan *ConsumerMessage {
	return c.messageChan
}

// Commit manually commits the current offset
func (c *Consumer) Commit(ctx context.Context) error {
	if atomic.LoadInt32(&c.closed) == 1 {
		return fmt.Errorf("consumer is closed")
	}

	c.offsetTracker.mutex.RLock()
	offset := c.offsetTracker.committedOffset
	c.offsetTracker.mutex.RUnlock()

	// TODO: Implement actual offset commit to broker
	fmt.Printf("Committing offset %d for consumer %s\n", offset, c.id)
	
	c.offsetTracker.mutex.Lock()
	c.offsetTracker.lastCommitTime = time.Now()
	c.offsetTracker.mutex.Unlock()

	return nil
}

// Seek seeks to a specific offset
func (c *Consumer) Seek(ctx context.Context, offset uint64) error {
	if atomic.LoadInt32(&c.closed) == 1 {
		return fmt.Errorf("consumer is closed")
	}

	// TODO: Implement seek functionality
	return fmt.Errorf("seek not implemented")
}

// SeekToTimestamp seeks to a specific timestamp
func (c *Consumer) SeekToTimestamp(ctx context.Context, timestamp time.Time) error {
	if atomic.LoadInt32(&c.closed) == 1 {
		return fmt.Errorf("consumer is closed")
	}

	// TODO: Implement timestamp-based seek
	return fmt.Errorf("seek to timestamp not implemented")
}

// Close closes the consumer
func (c *Consumer) Close() error {
	if !atomic.CompareAndSwapInt32(&c.closed, 0, 1) {
		return nil // Already closed
	}

	// Commit final offsets
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	
	c.Commit(ctx)
	
	// Close message channel
	close(c.messageChan)

	return nil
}

// startConsumption starts the background consumption loop
func (c *Consumer) startConsumption(ctx context.Context) {
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if atomic.LoadInt32(&c.closed) == 1 {
				return
			}

			// TODO: Implement actual message fetching from broker
			// For now, simulate receiving messages
			c.simulateMessageReceive()
		}
	}
}

// simulateMessageReceive simulates receiving messages (placeholder)
func (c *Consumer) simulateMessageReceive() {
	// This is a placeholder implementation
	// In real implementation, this would fetch messages from the broker
}

// startAutoCommit starts the auto-commit loop
func (c *Consumer) startAutoCommit(ctx context.Context) {
	ticker := time.NewTicker(c.config.AutoCommitInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if atomic.LoadInt32(&c.closed) == 1 {
				return
			}

			if err := c.Commit(ctx); err != nil {
				fmt.Printf("Auto-commit failed for consumer %s: %v\n", c.id, err)
			}
		}
	}
}

// handleAckRequests handles acknowledgment requests
func (c *Consumer) handleAckRequests(ctx context.Context, ackChan <-chan AckRequest) {
	for {
		select {
		case <-ctx.Done():
			return
		case ack := <-ackChan:
			c.offsetTracker.mutex.Lock()
			
			if ack.Success {
				delete(c.offsetTracker.pendingOffsets, ack.Offset)
				
				// Update committed offset to highest consecutive offset
				for {
					nextOffset := c.offsetTracker.committedOffset + 1
					if _, exists := c.offsetTracker.pendingOffsets[nextOffset]; !exists {
						c.offsetTracker.committedOffset = nextOffset
						delete(c.offsetTracker.pendingOffsets, nextOffset)
					} else {
						break
					}
				}
				
				atomic.AddUint64(&c.metrics.MessagesProcessed, 1)
			} else {
				atomic.AddUint64(&c.metrics.MessagesFailed, 1)
				// TODO: Handle failed message (retry, dead letter queue, etc.)
			}
			
			c.offsetTracker.mutex.Unlock()
		}
	}
}

// Metrics returns current consumer metrics
func (c *Consumer) Metrics() *ConsumerMetrics {
	c.offsetTracker.mutex.RLock()
	lag := c.offsetTracker.committedOffset // Simplified lag calculation
	c.offsetTracker.mutex.RUnlock()

	return &ConsumerMetrics{
		MessagesReceived:  atomic.LoadUint64(&c.metrics.MessagesReceived),
		MessagesProcessed: atomic.LoadUint64(&c.metrics.MessagesProcessed),
		MessagesFailed:    atomic.LoadUint64(&c.metrics.MessagesFailed),
		BytesReceived:     atomic.LoadUint64(&c.metrics.BytesReceived),
		Lag:               lag,
		LastReceiveTime:   c.metrics.LastReceiveTime,
		ProcessingTimeMs:  c.metrics.ProcessingTimeMs,
	}
}

// ID returns the consumer ID
func (c *Consumer) ID() string {
	return c.id
}

// Topic returns the topic name
func (c *Consumer) Topic() string {
	return c.topic
}

// ConsumerGroup returns the consumer group
func (c *Consumer) ConsumerGroup() string {
	return c.consumerGroup
}

// Config returns the consumer configuration
func (c *Consumer) Config() *ConsumerConfig {
	return c.config
}

// Ack acknowledges successful processing of a message
func (cm *ConsumerMessage) Ack() error {
	ackRequest := AckRequest{
		Offset:  cm.Message.Offset,
		Success: true,
	}

	select {
	case cm.ackChan <- ackRequest:
		return nil
	default:
		return fmt.Errorf("failed to send acknowledgment")
	}
}

// Nack negatively acknowledges a message (processing failed)
func (cm *ConsumerMessage) Nack() error {
	ackRequest := AckRequest{
		Offset:  cm.Message.Offset,
		Success: false,
	}

	select {
	case cm.ackChan <- ackRequest:
		return nil
	default:
		return fmt.Errorf("failed to send negative acknowledgment")
	}
}

// Partition returns the partition number
func (cm *ConsumerMessage) Partition() uint32 {
	return cm.partition
}

// RetryCount returns the number of retry attempts
func (cm *ConsumerMessage) RetryCount() int {
	return cm.retryCount
}

// ReceivedTime returns when the message was received
func (cm *ConsumerMessage) ReceivedTime() time.Time {
	return cm.receivedTime
}

// Age returns how long ago the message was received
func (cm *ConsumerMessage) Age() time.Duration {
	return time.Since(cm.receivedTime)
}