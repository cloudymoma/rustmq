package rustmq

import (
	"context"
	"fmt"
	"math"
	"math/rand"
	"sync"
	"sync/atomic"
	"time"
)

// MessageStream provides high-level streaming interface for real-time message processing
type MessageStream struct {
	config         *StreamConfig
	client         *Client
	consumers      []*Consumer
	producer       *Producer
	deadLetterProducer *Producer
	processor      MessageProcessor
	metrics        *StreamMetrics
	isRunning      int32
	ctx            context.Context
	cancel         context.CancelFunc
	wg             sync.WaitGroup
	batchCollector *BatchCollector
	windowManager  *WindowManager
	retryManager   *RetryManager
	circuitBreaker *CircuitBreaker
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
	Type             ErrorStrategyType `json:"type"`
	MaxAttempts      int               `json:"max_attempts,omitempty"`
	BackoffMs        int64             `json:"backoff_ms,omitempty"`
	MaxBackoffMs     int64             `json:"max_backoff_ms,omitempty"`
	BackoffMultiplier float64          `json:"backoff_multiplier,omitempty"`
	DeadLetterTopic  string            `json:"dead_letter_topic,omitempty"`
	CircuitBreaker   *CircuitBreakerConfig `json:"circuit_breaker,omitempty"`
}

// ErrorStrategyType represents different error handling strategies
type ErrorStrategyType string

const (
	ErrorStrategySkip       ErrorStrategyType = "skip"
	ErrorStrategyRetry      ErrorStrategyType = "retry"
	ErrorStrategyDeadLetter ErrorStrategyType = "dead_letter"
	ErrorStrategyStop       ErrorStrategyType = "stop"
)

// CircuitBreakerConfig represents circuit breaker configuration
type CircuitBreakerConfig struct {
	FailureThreshold int           `json:"failure_threshold"`
	SuccessThreshold int           `json:"success_threshold"`
	Timeout          time.Duration `json:"timeout"`
	MaxRequests      int           `json:"max_requests"`
}

// CircuitBreakerState represents circuit breaker states
type CircuitBreakerState int

const (
	Closed CircuitBreakerState = iota
	Open
	HalfOpen
)

// CircuitBreaker implements the circuit breaker pattern
type CircuitBreaker struct {
	config       *CircuitBreakerConfig
	state        CircuitBreakerState
	failureCount int
	successCount int
	lastFailTime time.Time
	mu           sync.RWMutex
}

// WindowManager manages time-based windows for windowed processing
type WindowManager struct {
	windows     map[string]*Window
	mu          sync.RWMutex
	windowSize  time.Duration
	slideInterval time.Duration
	sessionTimeout time.Duration
	cleanupTicker *time.Ticker
	stopCleanup   chan struct{}
}

// Window represents a time window containing messages
type Window struct {
	id        string
	startTime time.Time
	endTime   time.Time
	messages  []*Message
	lastActivity time.Time
	mu        sync.RWMutex
}

// BatchCollector manages message batching
type BatchCollector struct {
	messages     []*ConsumerMessage
	totalSize    int
	lastFlushTime time.Time
	mu           sync.RWMutex
	maxSize      int
	maxCount     int
	flushTimeout time.Duration
}

// RetryManager handles message retry logic with exponential backoff
type RetryManager struct {
	retryQueue map[string]*RetryItem
	mu         sync.RWMutex
	ticker     *time.Ticker
	stopChan   chan struct{}
}

// RetryItem represents a message waiting for retry
type RetryItem struct {
	message     *ConsumerMessage
	attempts    int
	nextRetry   time.Time
	backoffMs   int64
	maxAttempts int
}

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
			Type:              ErrorStrategySkip,
			MaxAttempts:       3,
			BackoffMs:         1000,
			MaxBackoffMs:      30000,
			BackoffMultiplier: 2.0,
			CircuitBreaker: &CircuitBreakerConfig{
				FailureThreshold: 5,
				SuccessThreshold: 3,
				Timeout:          60 * time.Second,
				MaxRequests:      10,
			},
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

	// Create dead letter producer if specified
	var deadLetterProducer *Producer
	if config.ErrorStrategy.DeadLetterTopic != "" {
		var err error
		deadLetterProducer, err = client.CreateProducer(config.ErrorStrategy.DeadLetterTopic)
		if err != nil {
			cancel()
			return nil, fmt.Errorf("failed to create dead letter producer for topic %s: %w", config.ErrorStrategy.DeadLetterTopic, err)
		}
	}

	// Initialize components
	batchCollector := &BatchCollector{
		maxSize:      config.Mode.BatchSize,
		maxCount:     config.Mode.BatchSize,
		flushTimeout: config.Mode.BatchTimeout,
		lastFlushTime: time.Now(),
	}

	windowManager := &WindowManager{
		windows:        make(map[string]*Window),
		windowSize:     config.Mode.WindowSize,
		slideInterval:  config.Mode.SlideInterval,
		sessionTimeout: config.Mode.WindowSize * 2, // Default session timeout
		stopCleanup:    make(chan struct{}),
	}

	retryManager := &RetryManager{
		retryQueue: make(map[string]*RetryItem),
		stopChan:   make(chan struct{}),
	}

	var circuitBreaker *CircuitBreaker
	if config.ErrorStrategy.CircuitBreaker != nil {
		circuitBreaker = &CircuitBreaker{
			config: config.ErrorStrategy.CircuitBreaker,
			state:  Closed,
		}
	}

	stream := &MessageStream{
		config:             config,
		client:             client,
		consumers:          consumers,
		producer:           producer,
		deadLetterProducer: deadLetterProducer,
		processor:          &DefaultProcessor{},
		metrics:            &StreamMetrics{},
		ctx:                ctx,
		cancel:             cancel,
		batchCollector:     batchCollector,
		windowManager:      windowManager,
		retryManager:       retryManager,
		circuitBreaker:     circuitBreaker,
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

	// Start retry manager if retry strategy is enabled
	if ms.config.ErrorStrategy.Type == ErrorStrategyRetry {
		ms.startRetryManager()
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

	// Close dead letter producer if exists
	if ms.deadLetterProducer != nil {
		if err := ms.deadLetterProducer.Close(); err != nil {
			return fmt.Errorf("failed to close dead letter producer: %w", err)
		}
	}

	// Stop retry manager
	if ms.retryManager != nil && ms.retryManager.ticker != nil {
		close(ms.retryManager.stopChan)
	}

	// Stop window manager cleanup
	if ms.windowManager != nil {
		close(ms.windowManager.stopCleanup)
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
	// Start batch flush timer
	ms.wg.Add(1)
	go func() {
		defer ms.wg.Done()
		ticker := time.NewTicker(ms.config.Mode.BatchTimeout)
		defer ticker.Stop()

		for {
			select {
			case <-ms.ctx.Done():
				return
			case <-ticker.C:
				ms.flushBatch(true) // Force flush on timeout
			}
		}
	}()

	// Start message consumption workers
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

				// Add message to batch
				ms.addToBatch(consumerMessage)
				<-semaphore // Release semaphore
			}
		}(consumer)
	}
}

// startWindowedProcessing starts windowed message processing
func (ms *MessageStream) startWindowedProcessing() {
	// Start window management
	ms.windowManager.start(ms.ctx, ms.wg.Add)

	// Start window processing timer
	ms.wg.Add(1)
	go func() {
		defer ms.wg.Done()
		ticker := time.NewTicker(ms.config.Mode.SlideInterval)
		defer ticker.Stop()

		for {
			select {
			case <-ms.ctx.Done():
				return
			case <-ticker.C:
				ms.processExpiredWindows()
			}
		}
	}()

	// Start message consumption workers
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

				// Add message to appropriate window
				ms.addToWindow(consumerMessage)
				<-semaphore // Release semaphore
			}
		}(consumer)
	}
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

// handleProcessingError handles processing errors with comprehensive error strategies
func (ms *MessageStream) handleProcessingError(consumerMessage *ConsumerMessage, err error) {
	atomic.AddUint64(&ms.metrics.MessagesFailed, 1)

	// Check circuit breaker
	if ms.circuitBreaker != nil {
		ms.circuitBreaker.recordFailure()
		if ms.circuitBreaker.getState() == Open {
			fmt.Printf("Circuit breaker is open, dropping message: %v\n", err)
			consumerMessage.Nack()
			return
		}
	}

	// Call error handler
	ms.processor.OnError(ms.ctx, consumerMessage.Message, err)

	switch ms.config.ErrorStrategy.Type {
	case ErrorStrategySkip:
		consumerMessage.Ack()
		atomic.AddUint64(&ms.metrics.MessagesSkipped, 1)
	case ErrorStrategyRetry:
		ms.scheduleRetry(consumerMessage, err)
	case ErrorStrategyDeadLetter:
		ms.sendToDeadLetterQueue(consumerMessage, err)
		consumerMessage.Ack()
	case ErrorStrategyStop:
		fmt.Printf("Stopping stream due to error: %v\n", err)
		ms.Stop(ms.ctx)
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

// Batch Processing Methods

// addToBatch adds a message to the current batch
func (ms *MessageStream) addToBatch(consumerMessage *ConsumerMessage) {
	ms.batchCollector.mu.Lock()
	defer ms.batchCollector.mu.Unlock()

	ms.batchCollector.messages = append(ms.batchCollector.messages, consumerMessage)
	ms.batchCollector.totalSize += consumerMessage.Message.TotalSize()

	// Check if we should flush the batch
	shouldFlush := len(ms.batchCollector.messages) >= ms.batchCollector.maxCount ||
		ms.batchCollector.totalSize >= ms.batchCollector.maxSize ||
		time.Since(ms.batchCollector.lastFlushTime) >= ms.batchCollector.flushTimeout

	if shouldFlush {
		ms.flushBatch(false)
	}
}

// flushBatch processes the current batch of messages
func (ms *MessageStream) flushBatch(force bool) {
	ms.batchCollector.mu.Lock()
	defer ms.batchCollector.mu.Unlock()

	if len(ms.batchCollector.messages) == 0 {
		return
	}

	if !force && time.Since(ms.batchCollector.lastFlushTime) < ms.batchCollector.flushTimeout {
		return
	}

	// Copy messages for processing
	batchMessages := make([]*Message, len(ms.batchCollector.messages))
	consumerMessages := make([]*ConsumerMessage, len(ms.batchCollector.messages))
	for i, cm := range ms.batchCollector.messages {
		batchMessages[i] = cm.Message
		consumerMessages[i] = cm
	}

	// Clear the batch
	ms.batchCollector.messages = ms.batchCollector.messages[:0]
	ms.batchCollector.totalSize = 0
	ms.batchCollector.lastFlushTime = time.Now()

	// Process batch asynchronously
	ms.wg.Add(1)
	go func() {
		defer ms.wg.Done()
		ms.processBatch(batchMessages, consumerMessages)
	}()
}

// processBatch processes a batch of messages
func (ms *MessageStream) processBatch(messages []*Message, consumerMessages []*ConsumerMessage) {
	startTime := time.Now()
	
	// Create processing context with timeout
	ctx, cancel := context.WithTimeout(ms.ctx, ms.config.ProcessingTimeout)
	defer cancel()

	// Check circuit breaker
	if ms.circuitBreaker != nil && ms.circuitBreaker.getState() == Open {
		fmt.Printf("Circuit breaker is open, dropping batch of %d messages\n", len(messages))
		for _, cm := range consumerMessages {
			cm.Nack()
		}
		return
	}

	// Process the batch
	results, err := ms.processor.ProcessBatch(ctx, messages)
	if err != nil {
		for _, cm := range consumerMessages {
			ms.handleProcessingError(cm, err)
		}
		return
	}

	// Send results to output topic if specified
	if ms.producer != nil && len(results) > 0 {
		for _, result := range results {
			if result != nil {
				_, sendErr := ms.producer.Send(ctx, result)
				if sendErr != nil {
					for _, cm := range consumerMessages {
						ms.handleProcessingError(cm, fmt.Errorf("failed to send result: %w", sendErr))
					}
					return
				}
			}
		}
	}

	// Acknowledge all messages
	for _, cm := range consumerMessages {
		if err := cm.Ack(); err != nil {
			fmt.Printf("Failed to acknowledge message: %v\n", err)
		}
	}

	// Record success in circuit breaker
	if ms.circuitBreaker != nil {
		ms.circuitBreaker.recordSuccess()
	}

	// Update metrics
	for range messages {
		ms.updateSuccessMetrics(startTime)
	}
}

// Windowed Processing Methods

// addToWindow adds a message to the appropriate window
func (ms *MessageStream) addToWindow(consumerMessage *ConsumerMessage) {
	ms.windowManager.addMessage(consumerMessage)
}

// processExpiredWindows processes windows that have expired
func (ms *MessageStream) processExpiredWindows() {
	expiredWindows := ms.windowManager.getExpiredWindows()
	for _, window := range expiredWindows {
		ms.wg.Add(1)
		go func(w *Window) {
			defer ms.wg.Done()
			ms.processWindow(w)
		}(window)
	}
}

// processWindow processes a single window
func (ms *MessageStream) processWindow(window *Window) {
	window.mu.RLock()
	messages := make([]*Message, len(window.messages))
	copy(messages, window.messages)
	window.mu.RUnlock()

	if len(messages) == 0 {
		return
	}

	startTime := time.Now()
	
	// Create processing context with timeout
	ctx, cancel := context.WithTimeout(ms.ctx, ms.config.ProcessingTimeout)
	defer cancel()

	// Check circuit breaker
	if ms.circuitBreaker != nil && ms.circuitBreaker.getState() == Open {
		fmt.Printf("Circuit breaker is open, dropping window with %d messages\n", len(messages))
		return
	}

	// Process the window
	results, err := ms.processor.ProcessBatch(ctx, messages)
	if err != nil {
		fmt.Printf("Error processing window: %v\n", err)
		if ms.circuitBreaker != nil {
			ms.circuitBreaker.recordFailure()
		}
		return
	}

	// Send results to output topic if specified
	if ms.producer != nil && len(results) > 0 {
		for _, result := range results {
			if result != nil {
				_, sendErr := ms.producer.Send(ctx, result)
				if sendErr != nil {
					fmt.Printf("Failed to send window result: %v\n", sendErr)
					if ms.circuitBreaker != nil {
						ms.circuitBreaker.recordFailure()
					}
					return
				}
			}
		}
	}

	// Record success in circuit breaker
	if ms.circuitBreaker != nil {
		ms.circuitBreaker.recordSuccess()
	}

	// Update metrics
	for range messages {
		ms.updateSuccessMetrics(startTime)
	}
}

// WindowManager methods

// start initializes the window manager
func (wm *WindowManager) start(ctx context.Context, addToWG func(int)) {
	addToWG(1)
	go func() {
		defer func() {
			if r := recover(); r != nil {
				fmt.Printf("Window manager panic: %v\n", r)
			}
		}()
		
		wm.cleanupTicker = time.NewTicker(time.Minute)
		defer wm.cleanupTicker.Stop()

		for {
			select {
			case <-ctx.Done():
				return
			case <-wm.stopCleanup:
				return
			case <-wm.cleanupTicker.C:
				wm.cleanupExpiredWindows()
			}
		}
	}()
}

// addMessage adds a message to the appropriate window
func (wm *WindowManager) addMessage(consumerMessage *ConsumerMessage) {
	wm.mu.Lock()
	defer wm.mu.Unlock()

	messageTime := consumerMessage.Message.Timestamp
	windowKey := wm.getWindowKey(messageTime)

	window, exists := wm.windows[windowKey]
	if !exists {
		startTime := wm.getWindowStart(messageTime)
		window = &Window{
			id:        windowKey,
			startTime: startTime,
			endTime:   startTime.Add(wm.windowSize),
			messages:  make([]*Message, 0),
		}
		wm.windows[windowKey] = window
	}

	window.mu.Lock()
	window.messages = append(window.messages, consumerMessage.Message)
	window.lastActivity = time.Now()
	window.mu.Unlock()
}

// getExpiredWindows returns windows that have expired and should be processed
func (wm *WindowManager) getExpiredWindows() []*Window {
	wm.mu.Lock()
	defer wm.mu.Unlock()

	now := time.Now()
	var expired []*Window

	for key, window := range wm.windows {
		window.mu.RLock()
		expired_condition := now.After(window.endTime) ||
			(wm.sessionTimeout > 0 && now.Sub(window.lastActivity) > wm.sessionTimeout)
		window.mu.RUnlock()

		if expired_condition {
			expired = append(expired, window)
			delete(wm.windows, key)
		}
	}

	return expired
}

// cleanupExpiredWindows removes old windows that are no longer needed
func (wm *WindowManager) cleanupExpiredWindows() {
	wm.mu.Lock()
	defer wm.mu.Unlock()

	now := time.Now()
	cutoff := now.Add(-wm.windowSize * 2) // Keep windows for 2x window size

	for key, window := range wm.windows {
		window.mu.RLock()
		lastActivity := window.lastActivity
		window.mu.RUnlock()

		if lastActivity.Before(cutoff) {
			delete(wm.windows, key)
		}
	}
}

// getWindowKey generates a key for the window based on message timestamp
func (wm *WindowManager) getWindowKey(messageTime time.Time) string {
	windowStart := wm.getWindowStart(messageTime)
	return fmt.Sprintf("%d", windowStart.Unix())
}

// getWindowStart calculates the start time of the window containing the message
func (wm *WindowManager) getWindowStart(messageTime time.Time) time.Time {
	if wm.slideInterval > 0 {
		// Sliding window
		slides := messageTime.Unix() / int64(wm.slideInterval.Seconds())
		return time.Unix(slides*int64(wm.slideInterval.Seconds()), 0)
	} else {
		// Tumbling window
		windows := messageTime.Unix() / int64(wm.windowSize.Seconds())
		return time.Unix(windows*int64(wm.windowSize.Seconds()), 0)
	}
}

// Error Handling and Retry Methods

// scheduleRetry schedules a message for retry with exponential backoff
func (ms *MessageStream) scheduleRetry(consumerMessage *ConsumerMessage, processingErr error) {
	if ms.retryManager == nil {
		fmt.Printf("Retry manager not initialized, dropping message\n")
		consumerMessage.Nack()
		return
	}

	messageID := consumerMessage.Message.ID
	retryCount := consumerMessage.RetryCount()

	if retryCount >= ms.config.ErrorStrategy.MaxAttempts {
		fmt.Printf("Max retry attempts reached for message %s, sending to dead letter queue\n", messageID)
		ms.sendToDeadLetterQueue(consumerMessage, processingErr)
		consumerMessage.Ack()
		return
	}

	// Calculate backoff with jitter
	backoffMs := ms.config.ErrorStrategy.BackoffMs
	for i := 0; i < retryCount; i++ {
		backoffMs = int64(float64(backoffMs) * ms.config.ErrorStrategy.BackoffMultiplier)
	}

	if backoffMs > ms.config.ErrorStrategy.MaxBackoffMs {
		backoffMs = ms.config.ErrorStrategy.MaxBackoffMs
	}

	// Add jitter (±25%)
	jitter := rand.Float64()*0.5 - 0.25
	backoffMs = int64(float64(backoffMs) * (1 + jitter))

	nextRetry := time.Now().Add(time.Duration(backoffMs) * time.Millisecond)

	ms.retryManager.mu.Lock()
	ms.retryManager.retryQueue[messageID] = &RetryItem{
		message:     consumerMessage,
		attempts:    retryCount + 1,
		nextRetry:   nextRetry,
		backoffMs:   backoffMs,
		maxAttempts: ms.config.ErrorStrategy.MaxAttempts,
	}
	ms.retryManager.mu.Unlock()

	// Start retry manager if not already running
	ms.startRetryManager()
	
	consumerMessage.Nack()
}

// startRetryManager starts the retry processing loop
func (ms *MessageStream) startRetryManager() {
	if ms.retryManager.ticker != nil {
		return // Already running
	}

	ms.retryManager.ticker = time.NewTicker(time.Second)
	ms.wg.Add(1)
	go func() {
		defer ms.wg.Done()
		defer ms.retryManager.ticker.Stop()

		for {
			select {
			case <-ms.ctx.Done():
				return
			case <-ms.retryManager.stopChan:
				return
			case <-ms.retryManager.ticker.C:
				ms.processRetryQueue()
			}
		}
	}()
}

// processRetryQueue processes messages that are ready for retry
func (ms *MessageStream) processRetryQueue() {
	ms.retryManager.mu.Lock()
	defer ms.retryManager.mu.Unlock()

	now := time.Now()
	for messageID, retryItem := range ms.retryManager.retryQueue {
		if now.After(retryItem.nextRetry) {
			// Remove from retry queue
			delete(ms.retryManager.retryQueue, messageID)

			// Retry processing
			ms.wg.Add(1)
			go func(item *RetryItem) {
				defer ms.wg.Done()
				ms.processSingleMessage(item.message)
			}(retryItem)
		}
	}
}

// sendToDeadLetterQueue sends a message to the dead letter queue
func (ms *MessageStream) sendToDeadLetterQueue(consumerMessage *ConsumerMessage, processingErr error) {
	if ms.deadLetterProducer == nil {
		fmt.Printf("Dead letter producer not configured, dropping message %s\n", consumerMessage.Message.ID)
		return
	}

	// Create dead letter message with error information
	deadLetterMessage := consumerMessage.Message.Clone()
	if deadLetterMessage.Headers == nil {
		deadLetterMessage.Headers = make(map[string]string)
	}
	deadLetterMessage.Headers["dlq_original_topic"] = consumerMessage.Message.Topic
	deadLetterMessage.Headers["dlq_error"] = processingErr.Error()
	deadLetterMessage.Headers["dlq_timestamp"] = time.Now().Format(time.RFC3339)
	deadLetterMessage.Headers["dlq_retry_count"] = fmt.Sprintf("%d", consumerMessage.RetryCount())

	ctx, cancel := context.WithTimeout(ms.ctx, 10*time.Second)
	defer cancel()

	_, err := ms.deadLetterProducer.Send(ctx, deadLetterMessage)
	if err != nil {
		fmt.Printf("Failed to send message to dead letter queue: %v\n", err)
	} else {
		fmt.Printf("Message %s sent to dead letter queue\n", consumerMessage.Message.ID)
	}
}

// Circuit Breaker Methods

// recordFailure records a failure in the circuit breaker
func (cb *CircuitBreaker) recordFailure() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	cb.failureCount++
	cb.lastFailTime = time.Now()

	if cb.state == Closed && cb.failureCount >= cb.config.FailureThreshold {
		cb.state = Open
		fmt.Printf("Circuit breaker opened after %d failures\n", cb.failureCount)
	}
}

// recordSuccess records a success in the circuit breaker
func (cb *CircuitBreaker) recordSuccess() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	if cb.state == HalfOpen {
		cb.successCount++
		if cb.successCount >= cb.config.SuccessThreshold {
			cb.state = Closed
			cb.failureCount = 0
			cb.successCount = 0
			fmt.Printf("Circuit breaker closed after %d successes\n", cb.successCount)
		}
	} else if cb.state == Closed {
		// Reset failure count on success
		if cb.failureCount > 0 {
			cb.failureCount = int(math.Max(0, float64(cb.failureCount-1)))
		}
	}
}

// getState returns the current state of the circuit breaker
func (cb *CircuitBreaker) getState() CircuitBreakerState {
	cb.mu.RLock()
	defer cb.mu.RUnlock()

	if cb.state == Open {
		// Check if we should transition to half-open
		if time.Since(cb.lastFailTime) > cb.config.Timeout {
			cb.mu.RUnlock()
			cb.mu.Lock()
			if cb.state == Open && time.Since(cb.lastFailTime) > cb.config.Timeout {
				cb.state = HalfOpen
				cb.successCount = 0
				fmt.Printf("Circuit breaker transitioned to half-open\n")
			}
			cb.mu.Unlock()
			cb.mu.RLock()
		}
	}

	return cb.state
}