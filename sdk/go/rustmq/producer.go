package rustmq

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/uuid"
)

// Producer sends messages to RustMQ topics
type Producer struct {
	id       string
	topic    string
	config   *ProducerConfig
	client   *Client
	sequence uint64
	metrics  *ProducerMetrics
	batcher  *MessageBatcher
	closed   int32
	mutex    sync.RWMutex
}

// ProducerConfig holds producer configuration
type ProducerConfig struct {
	ProducerID       string                 `json:"producer_id,omitempty"`
	BatchSize        int                    `json:"batch_size"`
	BatchTimeout     time.Duration          `json:"batch_timeout"`
	MaxMessageSize   int                    `json:"max_message_size"`
	AckLevel         AckLevel               `json:"ack_level"`
	Idempotent       bool                   `json:"idempotent"`
	Compression      *CompressionConfig     `json:"compression"`
	DefaultProps     map[string]string      `json:"default_properties,omitempty"`
}

// AckLevel represents acknowledgment levels
type AckLevel string

const (
	AckNone   AckLevel = "none"
	AckLeader AckLevel = "leader"
	AckAll    AckLevel = "all"
)

// MessageResult represents the result of sending a message
type MessageResult struct {
	MessageID string    `json:"message_id"`
	Topic     string    `json:"topic"`
	Partition uint32    `json:"partition"`
	Offset    uint64    `json:"offset"`
	Timestamp time.Time `json:"timestamp"`
}

// ProducerMetrics holds producer performance metrics
type ProducerMetrics struct {
	MessagesSent     uint64    `json:"messages_sent"`
	MessagesFailed   uint64    `json:"messages_failed"`
	BytesSent        uint64    `json:"bytes_sent"`
	BatchesSent      uint64    `json:"batches_sent"`
	AverageBatchSize float64   `json:"average_batch_size"`
	LastSendTime     time.Time `json:"last_send_time"`
}

// DefaultProducerConfig returns default producer configuration
func DefaultProducerConfig() *ProducerConfig {
	return &ProducerConfig{
		BatchSize:      100,
		BatchTimeout:   100 * time.Millisecond,
		MaxMessageSize: 1024 * 1024, // 1MB
		AckLevel:       AckLeader,
		Idempotent:     false,
		Compression: &CompressionConfig{
			Enabled:   false,
			Algorithm: CompressionNone,
			Level:     6,
			MinSize:   1024,
		},
		DefaultProps: make(map[string]string),
	}
}

// newProducer creates a new producer
func newProducer(client *Client, topic string, config *ProducerConfig) (*Producer, error) {
	if config.ProducerID == "" {
		config.ProducerID = fmt.Sprintf("producer-%s", uuid.New().String()[:8])
	}

	producer := &Producer{
		id:      config.ProducerID,
		topic:   topic,
		config:  config,
		client:  client,
		metrics: &ProducerMetrics{},
	}

	// Create message batcher
	producer.batcher = newMessageBatcher(producer, config.BatchSize, config.BatchTimeout)
	
	// Start background batching
	go producer.batcher.start(client.Context())

	return producer, nil
}

// Send sends a message and waits for the result
func (p *Producer) Send(ctx context.Context, message *Message) (*MessageResult, error) {
	if atomic.LoadInt32(&p.closed) == 1 {
		return nil, fmt.Errorf("producer is closed")
	}

	// Prepare message
	if err := p.prepareMessage(message); err != nil {
		return nil, fmt.Errorf("failed to prepare message: %w", err)
	}

	// Send through batcher
	resultChan := make(chan *MessageResult, 1)
	errorChan := make(chan error, 1)

	batchItem := &BatchItem{
		Message:    message,
		ResultChan: resultChan,
		ErrorChan:  errorChan,
	}

	select {
	case p.batcher.inputChan <- batchItem:
	case <-ctx.Done():
		return nil, ctx.Err()
	}

	// Wait for result
	select {
	case result := <-resultChan:
		atomic.AddUint64(&p.metrics.MessagesSent, 1)
		p.metrics.LastSendTime = time.Now()
		return result, nil
	case err := <-errorChan:
		atomic.AddUint64(&p.metrics.MessagesFailed, 1)
		return nil, err
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

// SendAsync sends a message without waiting for the result
func (p *Producer) SendAsync(ctx context.Context, message *Message, callback func(*MessageResult, error)) error {
	if atomic.LoadInt32(&p.closed) == 1 {
		return fmt.Errorf("producer is closed")
	}

	// Prepare message
	if err := p.prepareMessage(message); err != nil {
		return fmt.Errorf("failed to prepare message: %w", err)
	}

	// Send through batcher with callback
	batchItem := &BatchItem{
		Message:  message,
		Callback: callback,
	}

	select {
	case p.batcher.inputChan <- batchItem:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

// SendBatch sends multiple messages in a batch
func (p *Producer) SendBatch(ctx context.Context, messages []*Message) ([]*MessageResult, error) {
	if atomic.LoadInt32(&p.closed) == 1 {
		return nil, fmt.Errorf("producer is closed")
	}

	results := make([]*MessageResult, 0, len(messages))
	
	for _, message := range messages {
		result, err := p.Send(ctx, message)
		if err != nil {
			return results, err
		}
		results = append(results, result)
	}

	return results, nil
}

// Flush waits for all pending messages to be sent
func (p *Producer) Flush(ctx context.Context) error {
	if atomic.LoadInt32(&p.closed) == 1 {
		return fmt.Errorf("producer is closed")
	}

	return p.batcher.flush(ctx)
}

// Close closes the producer
func (p *Producer) Close() error {
	if !atomic.CompareAndSwapInt32(&p.closed, 0, 1) {
		return nil // Already closed
	}

	// Flush remaining messages
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	
	p.Flush(ctx)
	
	// Stop batcher
	p.batcher.stop()

	return nil
}

// prepareMessage prepares a message for sending
func (p *Producer) prepareMessage(message *Message) error {
	// Set producer ID
	message.ProducerID = p.id
	
	// Set topic if not already set
	if message.Topic == "" {
		message.Topic = p.topic
	}
	
	// Set sequence number
	sequence := atomic.AddUint64(&p.sequence, 1)
	message.Sequence = &sequence
	
	// Set timestamp if not set
	if message.Timestamp.IsZero() {
		message.Timestamp = time.Now()
	}
	
	// Apply default properties
	if message.Headers == nil {
		message.Headers = make(map[string]string)
	}
	
	for key, value := range p.config.DefaultProps {
		if _, exists := message.Headers[key]; !exists {
			message.Headers[key] = value
		}
	}
	
	// Validate message size
	if len(message.Payload) > p.config.MaxMessageSize {
		return fmt.Errorf("message size %d exceeds maximum %d", 
			len(message.Payload), p.config.MaxMessageSize)
	}

	return nil
}

// Metrics returns current producer metrics
func (p *Producer) Metrics() *ProducerMetrics {
	return &ProducerMetrics{
		MessagesSent:     atomic.LoadUint64(&p.metrics.MessagesSent),
		MessagesFailed:   atomic.LoadUint64(&p.metrics.MessagesFailed),
		BytesSent:        atomic.LoadUint64(&p.metrics.BytesSent),
		BatchesSent:      atomic.LoadUint64(&p.metrics.BatchesSent),
		AverageBatchSize: p.metrics.AverageBatchSize,
		LastSendTime:     p.metrics.LastSendTime,
	}
}

// ID returns the producer ID
func (p *Producer) ID() string {
	return p.id
}

// Topic returns the topic name
func (p *Producer) Topic() string {
	return p.topic
}

// Config returns the producer configuration
func (p *Producer) Config() *ProducerConfig {
	return p.config
}

// BatchItem represents an item in the message batch queue
type BatchItem struct {
	Message    *Message
	ResultChan chan *MessageResult
	ErrorChan  chan error
	Callback   func(*MessageResult, error)
}

// MessageBatcher handles batching of messages for efficient sending
type MessageBatcher struct {
	producer    *Producer
	batchSize   int
	batchTimeout time.Duration
	inputChan   chan *BatchItem
	flushChan   chan chan error
	stopChan    chan struct{}
}

// newMessageBatcher creates a new message batcher
func newMessageBatcher(producer *Producer, batchSize int, batchTimeout time.Duration) *MessageBatcher {
	return &MessageBatcher{
		producer:     producer,
		batchSize:    batchSize,
		batchTimeout: batchTimeout,
		inputChan:    make(chan *BatchItem, batchSize*2),
		flushChan:    make(chan chan error),
		stopChan:     make(chan struct{}),
	}
}

// start begins the batching process
func (b *MessageBatcher) start(ctx context.Context) {
	ticker := time.NewTicker(b.batchTimeout)
	defer ticker.Stop()

	batch := make([]*BatchItem, 0, b.batchSize)

	for {
		select {
		case <-ctx.Done():
			b.sendBatch(batch)
			return
		case <-b.stopChan:
			b.sendBatch(batch)
			return
		case item := <-b.inputChan:
			batch = append(batch, item)
			if len(batch) >= b.batchSize {
				b.sendBatch(batch)
				batch = batch[:0]
				ticker.Reset(b.batchTimeout)
			}
		case <-ticker.C:
			if len(batch) > 0 {
				b.sendBatch(batch)
				batch = batch[:0]
			}
		case responseChan := <-b.flushChan:
			if len(batch) > 0 {
				b.sendBatch(batch)
				batch = batch[:0]
			}
			responseChan <- nil
		}
	}
}

// sendBatch sends a batch of messages
func (b *MessageBatcher) sendBatch(batch []*BatchItem) {
	if len(batch) == 0 {
		return
	}

	// TODO: Implement actual batch sending to broker
	// For now, simulate successful sending
	for _, item := range batch {
		result := &MessageResult{
			MessageID: item.Message.ID,
			Topic:     item.Message.Topic,
			Partition: 0, // Would be assigned by broker
			Offset:    0, // Would be assigned by broker
			Timestamp: time.Now(),
		}

		if item.Callback != nil {
			item.Callback(result, nil)
		} else if item.ResultChan != nil {
			select {
			case item.ResultChan <- result:
			default:
			}
		}
	}

	// Update metrics
	atomic.AddUint64(&b.producer.metrics.BatchesSent, 1)
	atomic.AddUint64(&b.producer.metrics.BytesSent, uint64(len(batch)*1024)) // Estimate
	
	// Update average batch size
	totalBatches := atomic.LoadUint64(&b.producer.metrics.BatchesSent)
	totalMessages := atomic.LoadUint64(&b.producer.metrics.MessagesSent)
	if totalBatches > 0 {
		b.producer.metrics.AverageBatchSize = float64(totalMessages) / float64(totalBatches)
	}
}

// flush flushes any pending messages
func (b *MessageBatcher) flush(ctx context.Context) error {
	responseChan := make(chan error, 1)
	
	select {
	case b.flushChan <- responseChan:
		select {
		case err := <-responseChan:
			return err
		case <-ctx.Done():
			return ctx.Err()
		}
	case <-ctx.Done():
		return ctx.Err()
	}
}

// stop stops the batcher
func (b *MessageBatcher) stop() {
	close(b.stopChan)
}