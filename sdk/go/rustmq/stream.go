package rustmq

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

// MessageStream provides high-level streaming interface for real-time message processing
type MessageStream struct {
	config     *StreamConfig
	client     *Client
	consumers  []*Consumer
	producer   *Producer
	processor  MessageProcessor
	metrics    *StreamMetrics
	isRunning  int32
	ctx        context.Context
	cancel     context.CancelFunc
	wg         sync.WaitGroup
}

// StreamConfig holds configuration for message streams
type StreamConfig struct {
	InputTopics       []string          `json:"input_topics"`
	OutputTopic       string            `json:"output_topic,omitempty"`
	ConsumerGroup     string            `json:"consumer_group"`
	Parallelism       int               `json:"parallelism"`
	ProcessingTimeout time.Duration     `json:"processing_timeout"`
	ExactlyOnce       bool              `json:"exactly_once"`
	MaxInFlight       int               `json:"max_in_flight"`
	ErrorStrategy     ErrorStrategy     `json:"error_strategy"`
	Mode              StreamMode        `json:"mode"`
}

// StreamMode represents different stream processing modes
type StreamMode struct {
	Type         StreamModeType `json:"type"`
	BatchSize    int            `json:"batch_size,omitempty"`
	BatchTimeout time.Duration  `json:"batch_timeout,omitempty"`
	WindowSize   time.Duration  `json:"window_size,omitempty"`
	SlideInterval time.Duration `json:"slide_interval,omitempty"`
}

// StreamModeType represents stream processing mode types
type StreamModeType string

const (
	StreamModeIndividual StreamModeType = "individual"
	StreamModeBatch      StreamModeType = "batch"
	StreamModeWindowed   StreamModeType = "windowed"
)

// ErrorStrategy represents error handling strategies
type ErrorStrategy struct {
	Type         ErrorStrategyType `json:"type"`
	MaxAttempts  int               `json:"max_attempts,omitempty"`
	BackoffMs    int64             `json:"backoff_ms,omitempty"`
	DeadLetterTopic string         `json:"dead_letter_topic,omitempty"`
}

// ErrorStrategyType represents different error handling strategies
type ErrorStrategyType string

const (
	ErrorStrategySkip       ErrorStrategyType = "skip"
	ErrorStrategyRetry      ErrorStrategyType = "retry"
	ErrorStrategyDeadLetter ErrorStrategyType = "dead_letter"
	ErrorStrategyStop       ErrorStrategyType = "stop"
)

// MessageProcessor defines the interface for message processing logic
type MessageProcessor interface {
	// Process a single message
	Process(ctx context.Context, message *Message) (*Message, error)
	
	// Process a batch of messages
	ProcessBatch(ctx context.Context, messages []*Message) ([]*Message, error)
	
	// Called when stream starts
	OnStart(ctx context.Context) error
	
	// Called when stream stops
	OnStop(ctx context.Context) error
	
	// Called on processing error
	OnError(ctx context.Context, message *Message, err error) error
}

// StreamMetrics holds stream processing metrics
type StreamMetrics struct {
	MessagesProcessed     uint64    `json:"messages_processed"`
	MessagesFailed        uint64    `json:"messages_failed"`
	MessagesSkipped       uint64    `json:"messages_skipped"`
	ProcessingRate        float64   `json:"processing_rate"`        // messages per second
	AverageProcessingTime float64   `json:"average_processing_time"` // milliseconds
	ErrorRate             float64   `json:"error_rate"`
	LastProcessedTime     time.Time `json:"last_processed_time"`
}

// ProcessingContext provides context for message processing
type ProcessingContext struct {
	Message          *Message
	Partition        uint32
	Offset           uint64
	RetryCount       int
	ProcessingStart  time.Time
}

// DefaultStreamConfig returns a default stream configuration
func DefaultStreamConfig() *StreamConfig {
	return &StreamConfig{
		InputTopics:       []string{"input"},
		OutputTopic:       "output",
		ConsumerGroup:     "stream-processor",
		Parallelism:       1,
		ProcessingTimeout: 30 * time.Second,
		ExactlyOnce:       false,
		MaxInFlight:       100,
		ErrorStrategy: ErrorStrategy{
			Type: ErrorStrategySkip,
		},
		Mode: StreamMode{
			Type: StreamModeIndividual,
		},
	}
}

// newMessageStream creates a new message stream
func newMessageStream(client *Client, config *StreamConfig) (*MessageStream, error) {
	if config == nil {
		config = DefaultStreamConfig()
	}

	ctx, cancel := context.WithCancel(context.Background())

	// Create consumers for input topics
	consumers := make([]*Consumer, 0, len(config.InputTopics))
	for _, topic := range config.InputTopics {
		consumer, err := client.CreateConsumer(topic, config.ConsumerGroup)
		if err != nil {
			cancel()
			return nil, fmt.Errorf("failed to create consumer for topic %s: %w", topic, err)
		}
		consumers = append(consumers, consumer)
	}

	// Create producer for output topic if specified
	var producer *Producer
	if config.OutputTopic != "" {
		var err error
		producer, err = client.CreateProducer(config.OutputTopic)
		if err != nil {
			cancel()
			return nil, fmt.Errorf("failed to create producer for output topic %s: %w", config.OutputTopic, err)
		}
	}

	stream := &MessageStream{
		config:    config,
		client:    client,
		consumers: consumers,
		producer:  producer,
		processor: &DefaultProcessor{},
		metrics:   &StreamMetrics{},
		ctx:       ctx,
		cancel:    cancel,
	}

	return stream, nil
}

// WithProcessor sets a custom message processor
func (ms *MessageStream) WithProcessor(processor MessageProcessor) *MessageStream {
	ms.processor = processor
	return ms
}

// Start begins stream processing
func (ms *MessageStream) Start(ctx context.Context) error {
	if !atomic.CompareAndSwapInt32(&ms.isRunning, 0, 1) {
		return fmt.Errorf("stream is already running")
	}

	// Call processor start hook
	if err := ms.processor.OnStart(ctx); err != nil {
		atomic.StoreInt32(&ms.isRunning, 0)
		return fmt.Errorf("processor start failed: %w", err)
	}

	// Start processing based on mode
	switch ms.config.Mode.Type {
	case StreamModeIndividual:
		ms.startIndividualProcessing()
	case StreamModeBatch:
		ms.startBatchProcessing()
	case StreamModeWindowed:
		ms.startWindowedProcessing()
	default:
		atomic.StoreInt32(&ms.isRunning, 0)
		return fmt.Errorf("unknown stream mode: %s", ms.config.Mode.Type)
	}

	return nil
}

// Stop stops stream processing
func (ms *MessageStream) Stop(ctx context.Context) error {
	if !atomic.CompareAndSwapInt32(&ms.isRunning, 1, 0) {
		return nil // Already stopped
	}

	ms.cancel()
	ms.wg.Wait()

	// Call processor stop hook
	if err := ms.processor.OnStop(ctx); err != nil {
		return fmt.Errorf("processor stop failed: %w", err)
	}

	// Close consumers
	for _, consumer := range ms.consumers {
		if err := consumer.Close(); err != nil {
			// Log error but continue
			fmt.Printf("Error closing consumer: %v\n", err)
		}
	}

	// Close producer if exists
	if ms.producer != nil {
		if err := ms.producer.Close(); err != nil {
			return fmt.Errorf("failed to close producer: %w", err)
		}
	}

	return nil
}

// startIndividualProcessing starts individual message processing
func (ms *MessageStream) startIndividualProcessing() {
	semaphore := make(chan struct{}, ms.config.MaxInFlight)

	for _, consumer := range ms.consumers {
		ms.wg.Add(1)
		go func(c *Consumer) {
			defer ms.wg.Done()
			
			for atomic.LoadInt32(&ms.isRunning) == 1 {
				select {
				case <-ms.ctx.Done():
					return
				default:
				}

				// Acquire semaphore
				select {
				case semaphore <- struct{}{}:
				case <-ms.ctx.Done():
					return
				}

				// Receive message
				consumerMessage, err := c.Receive(ms.ctx)
				if err != nil {
					<-semaphore // Release semaphore
					if ms.ctx.Err() == nil {
						fmt.Printf("Error receiving message: %v\n", err)
						time.Sleep(100 * time.Millisecond)
					}
					continue
				}

				if consumerMessage == nil {
					<-semaphore // Release semaphore
					break
				}

				// Process message asynchronously
				ms.wg.Add(1)
				go func(cm *ConsumerMessage) {
					defer ms.wg.Done()
					defer func() { <-semaphore }() // Release semaphore
					
					ms.processSingleMessage(cm)
				}(consumerMessage)
			}
		}(consumer)
	}
}

// startBatchProcessing starts batch message processing
func (ms *MessageStream) startBatchProcessing() {
	// TODO: Implement batch processing
	fmt.Println("Batch processing not yet implemented")
}

// startWindowedProcessing starts windowed message processing
func (ms *MessageStream) startWindowedProcessing() {
	// TODO: Implement windowed processing
	fmt.Println("Windowed processing not yet implemented")
}

// processSingleMessage processes a single message
func (ms *MessageStream) processSingleMessage(consumerMessage *ConsumerMessage) {
	startTime := time.Now()
	
	// Create processing context with timeout
	ctx, cancel := context.WithTimeout(ms.ctx, ms.config.ProcessingTimeout)
	defer cancel()

	// Process the message
	result, err := ms.processor.Process(ctx, consumerMessage.Message)
	if err != nil {
		ms.handleProcessingError(consumerMessage, err)
		return
	}

	// Send result to output topic if specified and result is not nil
	if result != nil && ms.producer != nil {
		_, sendErr := ms.producer.Send(ctx, result)
		if sendErr != nil {
			ms.handleProcessingError(consumerMessage, fmt.Errorf("failed to send result: %w", sendErr))
			return
		}
	}

	// Acknowledge the message
	if err := consumerMessage.Ack(); err != nil {
		fmt.Printf("Failed to acknowledge message: %v\n", err)
	}

	// Update metrics
	ms.updateSuccessMetrics(startTime)
}

// handleProcessingError handles processing errors
func (ms *MessageStream) handleProcessingError(consumerMessage *ConsumerMessage, err error) {
	atomic.AddUint64(&ms.metrics.MessagesFailed, 1)

	// Call error handler
	ms.processor.OnError(ms.ctx, consumerMessage.Message, err)

	switch ms.config.ErrorStrategy.Type {
	case ErrorStrategySkip:
		consumerMessage.Ack()
		atomic.AddUint64(&ms.metrics.MessagesSkipped, 1)
	case ErrorStrategyRetry:
		// TODO: Implement retry logic
		consumerMessage.Nack()
	case ErrorStrategyDeadLetter:
		// TODO: Send to dead letter queue
		consumerMessage.Ack()
	case ErrorStrategyStop:
		// TODO: Stop the stream
		consumerMessage.Nack()
	}
}

// updateSuccessMetrics updates success metrics
func (ms *MessageStream) updateSuccessMetrics(startTime time.Time) {
	atomic.AddUint64(&ms.metrics.MessagesProcessed, 1)
	
	processingTime := time.Since(startTime).Seconds() * 1000 // Convert to milliseconds
	
	// Update average processing time (simplified)
	count := atomic.LoadUint64(&ms.metrics.MessagesProcessed)
	if count > 0 {
		ms.metrics.AverageProcessingTime = (ms.metrics.AverageProcessingTime*float64(count-1) + processingTime) / float64(count)
	}
	
	ms.metrics.LastProcessedTime = time.Now()
}

// Metrics returns current stream metrics
func (ms *MessageStream) Metrics() *StreamMetrics {
	// Calculate processing rate
	if !ms.metrics.LastProcessedTime.IsZero() {
		elapsed := time.Since(ms.metrics.LastProcessedTime).Seconds()
		if elapsed > 0 {
			ms.metrics.ProcessingRate = float64(atomic.LoadUint64(&ms.metrics.MessagesProcessed)) / elapsed
		}
	}

	// Calculate error rate
	totalMessages := atomic.LoadUint64(&ms.metrics.MessagesProcessed) + atomic.LoadUint64(&ms.metrics.MessagesFailed)
	if totalMessages > 0 {
		ms.metrics.ErrorRate = float64(atomic.LoadUint64(&ms.metrics.MessagesFailed)) / float64(totalMessages) * 100
	}

	return &StreamMetrics{
		MessagesProcessed:     atomic.LoadUint64(&ms.metrics.MessagesProcessed),
		MessagesFailed:        atomic.LoadUint64(&ms.metrics.MessagesFailed),
		MessagesSkipped:       atomic.LoadUint64(&ms.metrics.MessagesSkipped),
		ProcessingRate:        ms.metrics.ProcessingRate,
		AverageProcessingTime: ms.metrics.AverageProcessingTime,
		ErrorRate:             ms.metrics.ErrorRate,
		LastProcessedTime:     ms.metrics.LastProcessedTime,
	}
}

// IsRunning checks if the stream is running
func (ms *MessageStream) IsRunning() bool {
	return atomic.LoadInt32(&ms.isRunning) == 1
}

// DefaultProcessor is a default no-op message processor
type DefaultProcessor struct{}

// Process implements MessageProcessor interface
func (dp *DefaultProcessor) Process(ctx context.Context, message *Message) (*Message, error) {
	// Default: pass through the message unchanged
	return message, nil
}

// ProcessBatch implements MessageProcessor interface
func (dp *DefaultProcessor) ProcessBatch(ctx context.Context, messages []*Message) ([]*Message, error) {
	results := make([]*Message, len(messages))
	for i, message := range messages {
		result, err := dp.Process(ctx, message)
		if err != nil {
			return results[:i], err
		}
		results[i] = result
	}
	return results, nil
}

// OnStart implements MessageProcessor interface
func (dp *DefaultProcessor) OnStart(ctx context.Context) error {
	return nil
}

// OnStop implements MessageProcessor interface
func (dp *DefaultProcessor) OnStop(ctx context.Context) error {
	return nil
}

// OnError implements MessageProcessor interface
func (dp *DefaultProcessor) OnError(ctx context.Context, message *Message, err error) error {
	fmt.Printf("Processing error for message %s: %v\n", message.ID, err)
	return nil
}