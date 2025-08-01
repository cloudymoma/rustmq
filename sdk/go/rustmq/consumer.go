package rustmq

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
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
	ackChan       chan AckRequest
	retryQueue    *RetryQueue
	fetchCtx      context.Context
	fetchCancel   context.CancelFunc
	closed        int32
	mutex         sync.RWMutex
	partitions    []uint32
	lastFetchTime time.Time
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
	highWaterMark   uint64
	lowWaterMark    uint64
	mutex           sync.RWMutex
}

// RetryQueue manages failed messages for retry processing
type RetryQueue struct {
	failedMessages map[string]*FailedMessage
	mutex          sync.RWMutex
	retryInterval  time.Duration
	maxRetries     int
}

// FailedMessage represents a message that failed processing
type FailedMessage struct {
	Message       *ConsumerMessage
	RetryCount    int
	FirstFailTime time.Time
	LastFailTime  time.Time
	Error         error
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

	fetchCtx, fetchCancel := context.WithCancel(client.Context())

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
		ackChan:     make(chan AckRequest, config.FetchSize),
		retryQueue: &RetryQueue{
			failedMessages: make(map[string]*FailedMessage),
			retryInterval:  5 * time.Second,
			maxRetries:     config.MaxRetryAttempts,
		},
		fetchCtx:    fetchCtx,
		fetchCancel: fetchCancel,
		partitions:  []uint32{}, // Will be populated by partition assignment
	}

	// Join consumer group and get partition assignment
	if err := consumer.joinConsumerGroup(); err != nil {
		fetchCancel()
		return nil, fmt.Errorf("failed to join consumer group: %w", err)
	}

	// Start background consumption
	go consumer.startConsumption()

	// Start acknowledgment handler
	go consumer.handleAckRequests(consumer.fetchCtx, consumer.ackChan)

	// Start auto-commit if enabled
	if config.EnableAutoCommit {
		go consumer.startAutoCommit()
	}

	// Start retry handler
	go consumer.startRetryHandler()

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

	// Build commit request
	offsetsMap := make(map[string]OffsetAndMetadata)
	for _, partition := range c.partitions {
		key := fmt.Sprintf("%s:%d", c.topic, partition)
		offsetsMap[key] = NewOffsetAndMetadata(offset, nil)
	}

	commitRequest := CommitRequest{
		Type:          "commit_offset",
		ConsumerGroup: c.consumerGroup,
		ConsumerID:    c.id,
		Offsets:       offsetsMap,
		Timestamp:     time.Now(),
	}

	// Serialize request
	requestData, err := json.Marshal(commitRequest)
	if err != nil {
		return fmt.Errorf("failed to marshal commit request: %w", err)
	}

	// Send commit request to broker
	responseData, err := c.client.connection.sendRequest(requestData)
	if err != nil {
		return fmt.Errorf("failed to send commit request: %w", err)
	}

	// Parse response
	var response CommitResponse
	if err := json.Unmarshal(responseData, &response); err != nil {
		return fmt.Errorf("failed to parse commit response: %w", err)
	}

	if !response.Success {
		errorMsg := "unknown error"
		if response.Error != nil {
			errorMsg = *response.Error
		}
		return fmt.Errorf("commit failed: %s", errorMsg)
	}

	// Update commit time on success
	c.offsetTracker.mutex.Lock()
	c.offsetTracker.lastCommitTime = time.Now()
	c.offsetTracker.mutex.Unlock()

	log.Printf("Successfully committed offset %d for consumer %s", offset, c.id)
	return nil
}

// Seek seeks to a specific offset
func (c *Consumer) Seek(ctx context.Context, offset uint64) error {
	if atomic.LoadInt32(&c.closed) == 1 {
		return fmt.Errorf("consumer is closed")
	}

	// Seek for all assigned partitions
	for _, partition := range c.partitions {
		if err := c.seekPartition(ctx, partition, &offset, nil); err != nil {
			return fmt.Errorf("failed to seek partition %d: %w", partition, err)
		}
	}

	log.Printf("Successfully sought to offset %d for consumer %s", offset, c.id)
	return nil
}

// SeekToTimestamp seeks to a specific timestamp
func (c *Consumer) SeekToTimestamp(ctx context.Context, timestamp time.Time) error {
	if atomic.LoadInt32(&c.closed) == 1 {
		return fmt.Errorf("consumer is closed")
	}

	// Seek for all assigned partitions
	for _, partition := range c.partitions {
		if err := c.seekPartition(ctx, partition, nil, &timestamp); err != nil {
			return fmt.Errorf("failed to seek partition %d to timestamp: %w", partition, err)
		}
	}

	log.Printf("Successfully sought to timestamp %v for consumer %s", timestamp, c.id)
	return nil
}

// Close closes the consumer
func (c *Consumer) Close() error {
	if !atomic.CompareAndSwapInt32(&c.closed, 0, 1) {
		return nil // Already closed
	}

	// Cancel fetch context to stop all background goroutines
	c.fetchCancel()

	// Commit final offsets
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	
	if err := c.Commit(ctx); err != nil {
		log.Printf("Failed to commit final offsets: %v", err)
	}
	
	// Close channels
	close(c.messageChan)
	close(c.ackChan)

	log.Printf("Consumer %s closed", c.id)
	return nil
}

// startConsumption starts the background consumption loop
func (c *Consumer) startConsumption() {
	ticker := time.NewTicker(time.Duration(c.config.FetchTimeout.Nanoseconds() / 2)) // Fetch at half the timeout interval
	defer ticker.Stop()

	for {
		select {
		case <-c.fetchCtx.Done():
			return
		case <-ticker.C:
			if atomic.LoadInt32(&c.closed) == 1 {
				return
			}

			// Fetch messages from broker
			if err := c.fetchMessages(); err != nil {
				log.Printf("Error fetching messages for consumer %s: %v", c.id, err)
				// Implement exponential backoff on error
				time.Sleep(time.Second)
			}
		}
	}
}

// fetchMessages fetches messages from the broker
func (c *Consumer) fetchMessages() error {
	// Check if we have channel space
	if len(c.messageChan) >= cap(c.messageChan)-10 { // Leave some buffer
		return nil // Channel is nearly full, skip this fetch
	}

	// Get current offset to fetch from
	c.offsetTracker.mutex.RLock()
	fromOffset := c.offsetTracker.committedOffset + 1
	c.offsetTracker.mutex.RUnlock()

	// Build fetch request
	fetchRequest := FetchRequest{
		Type:          "fetch_messages",
		Topic:         c.topic,
		ConsumerGroup: c.consumerGroup,
		ConsumerID:    c.id,
		MaxMessages:   c.config.FetchSize,
		MaxBytes:      1024 * 1024, // 1MB max
		TimeoutMs:     c.config.FetchTimeout.Milliseconds(),
		FromOffset:    &fromOffset,
		Timestamp:     time.Now(),
	}

	// Serialize request
	requestData, err := json.Marshal(fetchRequest)
	if err != nil {
		return fmt.Errorf("failed to marshal fetch request: %w", err)
	}

	// Send fetch request to broker
	responseData, err := c.client.connection.sendRequest(requestData)
	if err != nil {
		return fmt.Errorf("failed to send fetch request: %w", err)
	}

	// Parse response
	var response FetchResponse
	if err := json.Unmarshal(responseData, &response); err != nil {
		return fmt.Errorf("failed to parse fetch response: %w", err)
	}

	if !response.Success {
		if response.Error != nil && *response.Error != "no_messages" {
			return fmt.Errorf("fetch failed: %s", *response.Error)
		}
		return nil // No messages available
	}

	// Process fetched messages
	for _, message := range response.Messages {
		consumerMessage := &ConsumerMessage{
			Message:      message,
			ackChan:      c.ackChan,
			partition:    response.PartitionID,
			retryCount:   0,
			receivedTime: time.Now(),
		}

		// Try to send message to channel
		select {
		case c.messageChan <- consumerMessage:
			// Mark offset as pending
			c.offsetTracker.mutex.Lock()
			c.offsetTracker.pendingOffsets[message.Offset] = true
			// Update watermarks if available
			if response.Watermarks != nil {
				c.offsetTracker.lowWaterMark = response.Watermarks.Low
				c.offsetTracker.highWaterMark = response.Watermarks.High
			}
			c.offsetTracker.mutex.Unlock()
		default:
			// Channel is full, put message back for next fetch
			log.Printf("Message channel full for consumer %s, message %s will be refetched", c.id, message.ID)
			return nil
		}
	}

	c.lastFetchTime = time.Now()
	return nil
}

// startAutoCommit starts the auto-commit loop
func (c *Consumer) startAutoCommit() {
	ticker := time.NewTicker(c.config.AutoCommitInterval)
	defer ticker.Stop()

	for {
		select {
		case <-c.fetchCtx.Done():
			return
		case <-ticker.C:
			if atomic.LoadInt32(&c.closed) == 1 {
				return
			}

			ctx, cancel := context.WithTimeout(c.fetchCtx, 5*time.Second)
			if err := c.Commit(ctx); err != nil {
				log.Printf("Auto-commit failed for consumer %s: %v", c.id, err)
			}
			cancel()
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
				
				// Handle failed message - add to retry queue
				c.handleFailedMessage(ack.Offset)
			}
			
			c.offsetTracker.mutex.Unlock()
		}
	}
}

// Metrics returns current consumer metrics
func (c *Consumer) Metrics() *ConsumerMetrics {
	c.offsetTracker.mutex.RLock()
	// Calculate lag based on high watermark vs committed offset
	lag := uint64(0)
	if c.offsetTracker.highWaterMark > c.offsetTracker.committedOffset {
		lag = c.offsetTracker.highWaterMark - c.offsetTracker.committedOffset
	}
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

// Helper methods for consumer functionality

// joinConsumerGroup joins the consumer group and gets partition assignment
func (c *Consumer) joinConsumerGroup() error {
	joinRequest := ConsumerGroupJoinRequest{
		Type:          "join_consumer_group",
		ConsumerGroup: c.consumerGroup,
		ConsumerID:    c.id,
		Topics:        []string{c.topic},
		Metadata:      map[string]string{"client_version": "go-sdk-1.0"},
		Timestamp:     time.Now(),
	}

	// Serialize request
	requestData, err := json.Marshal(joinRequest)
	if err != nil {
		return fmt.Errorf("failed to marshal join request: %w", err)
	}

	// Send join request to broker
	responseData, err := c.client.connection.sendRequest(requestData)
	if err != nil {
		return fmt.Errorf("failed to send join request: %w", err)
	}

	// Parse response
	var response ConsumerGroupJoinResponse
	if err := json.Unmarshal(responseData, &response); err != nil {
		return fmt.Errorf("failed to parse join response: %w", err)
	}

	if !response.Success {
		errorMsg := "unknown error"
		if response.Error != nil {
			errorMsg = *response.Error
		}
		return fmt.Errorf("failed to join consumer group: %s", errorMsg)
	}

	// Extract assigned partitions
	for _, tp := range response.Assignment {
		if tp.Topic == c.topic {
			c.partitions = append(c.partitions, tp.Partition)
		}
	}

	log.Printf("Consumer %s joined group %s with partitions: %v", c.id, c.consumerGroup, c.partitions)
	return nil
}

// seekPartition seeks a specific partition to offset or timestamp
func (c *Consumer) seekPartition(ctx context.Context, partition uint32, offset *uint64, timestamp *time.Time) error {
	seekRequest := SeekRequest{
		Type:          "seek_partition",
		Topic:         c.topic,
		ConsumerGroup: c.consumerGroup,
		ConsumerID:    c.id,
		Partition:     partition,
		Offset:        offset,
		Timestamp:     timestamp,
	}

	// Serialize request
	requestData, err := json.Marshal(seekRequest)
	if err != nil {
		return fmt.Errorf("failed to marshal seek request: %w", err)
	}

	// Send seek request to broker
	responseData, err := c.client.connection.sendRequest(requestData)
	if err != nil {
		return fmt.Errorf("failed to send seek request: %w", err)
	}

	// Parse response
	var response SeekResponse
	if err := json.Unmarshal(responseData, &response); err != nil {
		return fmt.Errorf("failed to parse seek response: %w", err)
	}

	if !response.Success {
		errorMsg := "unknown error"
		if response.Error != nil {
			errorMsg = *response.Error
		}
		return fmt.Errorf("seek failed: %s", errorMsg)
	}

	// Update offset tracker
	c.offsetTracker.mutex.Lock()
	c.offsetTracker.committedOffset = response.NewOffset
	// Clear pending offsets as they're now invalid
	c.offsetTracker.pendingOffsets = make(map[uint64]bool)
	c.offsetTracker.mutex.Unlock()

	return nil
}

// handleFailedMessage handles a failed message by adding it to retry queue
func (c *Consumer) handleFailedMessage(offset uint64) {
	// Find the message in channel or create a placeholder for retry
	key := fmt.Sprintf("%s:%d", c.topic, offset)
	
	c.retryQueue.mutex.Lock()
	defer c.retryQueue.mutex.Unlock()

	if failedMsg, exists := c.retryQueue.failedMessages[key]; exists {
		// Increment retry count
		failedMsg.RetryCount++
		failedMsg.LastFailTime = time.Now()
		
		// Check if max retries exceeded
		if failedMsg.RetryCount >= c.retryQueue.maxRetries {
			// Send to dead letter queue if configured
			if c.config.DeadLetterQueue != "" {
				go c.sendToDeadLetterQueue(failedMsg)
			}
			// Remove from retry queue
			delete(c.retryQueue.failedMessages, key)
			log.Printf("Message at offset %d exceeded max retries, sent to DLQ", offset)
		}
	} else {
		// First failure
		c.retryQueue.failedMessages[key] = &FailedMessage{
			RetryCount:    1,
			FirstFailTime: time.Now(),
			LastFailTime:  time.Now(),
		}
	}
}

// startRetryHandler manages retry processing for failed messages
func (c *Consumer) startRetryHandler() {
	ticker := time.NewTicker(c.retryQueue.retryInterval)
	defer ticker.Stop()

	for {
		select {
		case <-c.fetchCtx.Done():
			return
		case <-ticker.C:
			if atomic.LoadInt32(&c.closed) == 1 {
				return
			}

			c.processRetries()
		}
	}
}

// processRetries processes messages in the retry queue
func (c *Consumer) processRetries() {
	c.retryQueue.mutex.RLock()
	retriesToProcess := make([]*FailedMessage, 0)
	for _, failedMsg := range c.retryQueue.failedMessages {
		// Check if enough time has passed for retry
		if time.Since(failedMsg.LastFailTime) >= c.retryQueue.retryInterval {
			retriesToProcess = append(retriesToProcess, failedMsg)
		}
	}
	c.retryQueue.mutex.RUnlock()

	// Process retries
	for _, failedMsg := range retriesToProcess {
		if len(c.messageChan) < cap(c.messageChan) {
			// Update retry count and time
			failedMsg.RetryCount++
			failedMsg.LastFailTime = time.Now()
			
			// Try to put message back in channel for retry
			select {
			case c.messageChan <- failedMsg.Message:
				log.Printf("Retrying message at offset %d (attempt %d)", 
					failedMsg.Message.Message.Offset, failedMsg.RetryCount)
			default:
				// Channel full, will retry later
			}
		}
	}
}

// sendToDeadLetterQueue sends a failed message to dead letter queue
func (c *Consumer) sendToDeadLetterQueue(failedMsg *FailedMessage) {
	if c.config.DeadLetterQueue == "" {
		return
	}

	// Create a producer for the DLQ if we don't have one
	producer, err := c.client.CreateProducer(c.config.DeadLetterQueue)
	if err != nil {
		log.Printf("Failed to create DLQ producer: %v", err)
		return
	}

	// Add DLQ headers
	dlqMessage := failedMsg.Message.Message.Clone()
	if dlqMessage.Headers == nil {
		dlqMessage.Headers = make(map[string]string)
	}
	dlqMessage.Headers["dlq_source_topic"] = c.topic
	dlqMessage.Headers["dlq_consumer_group"] = c.consumerGroup
	dlqMessage.Headers["dlq_retry_count"] = fmt.Sprintf("%d", failedMsg.RetryCount)
	dlqMessage.Headers["dlq_first_fail_time"] = failedMsg.FirstFailTime.Format(time.RFC3339)
	dlqMessage.Headers["dlq_last_fail_time"] = failedMsg.LastFailTime.Format(time.RFC3339)

	// Send to DLQ
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	_, err = producer.Send(ctx, dlqMessage)
	if err != nil {
		log.Printf("Failed to send message to DLQ: %v", err)
	} else {
		log.Printf("Message at offset %d sent to DLQ after %d retries", 
			dlqMessage.Offset, failedMsg.RetryCount)
	}
}