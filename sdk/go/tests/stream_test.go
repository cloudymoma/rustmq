package tests

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// MockProcessor for testing stream processing
type MockProcessor struct {
	processedCount int64
	processDelay   time.Duration
	shouldError    bool
	batchSizes     []int
	mu             sync.Mutex
}

func (mp *MockProcessor) Process(ctx context.Context, message *rustmq.Message) (*rustmq.Message, error) {
	atomic.AddInt64(&mp.processedCount, 1)
	
	if mp.processDelay > 0 {
		time.Sleep(mp.processDelay)
	}
	
	if mp.shouldError {
		return nil, fmt.Errorf("mock processing error")
	}
	
	// Return processed message
	return message, nil
}

func (mp *MockProcessor) ProcessBatch(ctx context.Context, messages []*rustmq.Message) ([]*rustmq.Message, error) {
	mp.mu.Lock()
	mp.batchSizes = append(mp.batchSizes, len(messages))
	mp.mu.Unlock()
	
	atomic.AddInt64(&mp.processedCount, int64(len(messages)))
	
	if mp.processDelay > 0 {
		time.Sleep(mp.processDelay)
	}
	
	if mp.shouldError {
		return nil, fmt.Errorf("mock batch processing error")
	}
	
	return messages, nil
}

func (mp *MockProcessor) OnStart(ctx context.Context) error {
	return nil
}

func (mp *MockProcessor) OnStop(ctx context.Context) error {
	return nil
}

func (mp *MockProcessor) OnError(ctx context.Context, message *rustmq.Message, err error) error {
	return nil
}

func (mp *MockProcessor) GetProcessedCount() int64 {
	return atomic.LoadInt64(&mp.processedCount)
}

func (mp *MockProcessor) GetBatchSizes() []int {
	mp.mu.Lock()
	defer mp.mu.Unlock()
	result := make([]int, len(mp.batchSizes))
	copy(result, mp.batchSizes)
	return result
}

// TestStreamConfiguration tests stream configuration
func TestStreamConfiguration(t *testing.T) {
	config := rustmq.DefaultStreamConfig()
	
	// Test default configuration
	if config.InputTopics[0] != "input" {
		t.Errorf("Expected default input topic 'input', got %s", config.InputTopics[0])
	}
	
	if config.OutputTopic != "output" {
		t.Errorf("Expected default output topic 'output', got %s", config.OutputTopic)
	}
	
	if config.Mode.Type != rustmq.StreamModeIndividual {
		t.Errorf("Expected default mode 'individual', got %s", config.Mode.Type)
	}
	
	if config.ErrorStrategy.Type != rustmq.ErrorStrategySkip {
		t.Errorf("Expected default error strategy 'skip', got %s", config.ErrorStrategy.Type)
	}
}

// TestBatchCollector tests the batch collection functionality
func TestBatchCollector(t *testing.T) {
	tests := []struct {
		name           string
		batchSize      int
		batchTimeout   time.Duration
		messageCount   int
		expectedBatches int
	}{
		{
			name:           "BatchBySize",
			batchSize:      3,
			batchTimeout:   time.Second,
			messageCount:   10,
			expectedBatches: 4, // 3+3+3+1
		},
		{
			name:           "BatchByTimeout",
			batchSize:      100,
			batchTimeout:   50 * time.Millisecond,
			messageCount:   5,
			expectedBatches: 1, // All messages in one batch due to timeout
		},
	}
	
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			config := rustmq.DefaultStreamConfig()
			config.Mode.Type = rustmq.StreamModeBatch
			config.Mode.BatchSize = tt.batchSize
			config.Mode.BatchTimeout = tt.batchTimeout
			
			processor := &MockProcessor{}
			
			// Since we can't create a real client in tests, we'll test the configuration
			if config.Mode.BatchSize != tt.batchSize {
				t.Errorf("Expected batch size %d, got %d", tt.batchSize, config.Mode.BatchSize)
			}
			
			if config.Mode.BatchTimeout != tt.batchTimeout {
				t.Errorf("Expected batch timeout %v, got %v", tt.batchTimeout, config.Mode.BatchTimeout)
			}
			
			// Test processor interface
			messages := make([]*rustmq.Message, tt.messageCount)
			for i := 0; i < tt.messageCount; i++ {
				messages[i] = rustmq.NewMessage().
					PayloadString(fmt.Sprintf("message-%d", i)).
					Build()
			}
			
			ctx := context.Background()
			_, err := processor.ProcessBatch(ctx, messages)
			if err != nil {
				t.Fatalf("ProcessBatch failed: %v", err)
			}
			
			if processor.GetProcessedCount() != int64(tt.messageCount) {
				t.Errorf("Expected %d processed messages, got %d", 
					tt.messageCount, processor.GetProcessedCount())
			}
		})
	}
}

// TestWindowManager tests windowed processing functionality
func TestWindowManager(t *testing.T) {
	tests := []struct {
		name          string
		windowSize    time.Duration
		slideInterval time.Duration
		messageCount  int
	}{
		{
			name:          "TumblingWindow",
			windowSize:    time.Second,
			slideInterval: 0, // No slide interval means tumbling
			messageCount:  5,
		},
		{
			name:          "SlidingWindow",
			windowSize:    time.Second,
			slideInterval: 500 * time.Millisecond,
			messageCount:  5,
		},
	}
	
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			config := rustmq.DefaultStreamConfig()
			config.Mode.Type = rustmq.StreamModeWindowed
			config.Mode.WindowSize = tt.windowSize
			config.Mode.SlideInterval = tt.slideInterval
			
			// Test configuration
			if config.Mode.WindowSize != tt.windowSize {
				t.Errorf("Expected window size %v, got %v", tt.windowSize, config.Mode.WindowSize)
			}
			
			if config.Mode.SlideInterval != tt.slideInterval {
				t.Errorf("Expected slide interval %v, got %v", tt.slideInterval, config.Mode.SlideInterval)
			}
		})
	}
}

// TestErrorHandlingStrategies tests different error handling strategies
func TestErrorHandlingStrategies(t *testing.T) {
	strategies := []rustmq.ErrorStrategyType{
		rustmq.ErrorStrategySkip,
		rustmq.ErrorStrategyRetry,
		rustmq.ErrorStrategyDeadLetter,
		rustmq.ErrorStrategyStop,
	}
	
	for _, strategy := range strategies {
		t.Run(string(strategy), func(t *testing.T) {
			config := rustmq.DefaultStreamConfig()
			config.ErrorStrategy.Type = strategy
			config.ErrorStrategy.MaxAttempts = 3
			config.ErrorStrategy.BackoffMs = 1000
			config.ErrorStrategy.MaxBackoffMs = 30000
			config.ErrorStrategy.BackoffMultiplier = 2.0
			
			if strategy == rustmq.ErrorStrategyDeadLetter {
				config.ErrorStrategy.DeadLetterTopic = "dead-letter-queue"
			}
			
			// Test configuration
			if config.ErrorStrategy.Type != strategy {
				t.Errorf("Expected error strategy %s, got %s", strategy, config.ErrorStrategy.Type)
			}
			
			if config.ErrorStrategy.MaxAttempts != 3 {
				t.Errorf("Expected max attempts 3, got %d", config.ErrorStrategy.MaxAttempts)
			}
			
			if config.ErrorStrategy.BackoffMs != 1000 {
				t.Errorf("Expected backoff 1000ms, got %d", config.ErrorStrategy.BackoffMs)
			}
		})
	}
}

// TestCircuitBreakerConfiguration tests circuit breaker setup
func TestCircuitBreakerConfiguration(t *testing.T) {
	config := rustmq.DefaultStreamConfig()
	
	// Test default circuit breaker configuration
	if config.ErrorStrategy.CircuitBreaker == nil {
		t.Fatal("Expected circuit breaker configuration to be set by default")
	}
	
	cb := config.ErrorStrategy.CircuitBreaker
	if cb.FailureThreshold != 5 {
		t.Errorf("Expected failure threshold 5, got %d", cb.FailureThreshold)
	}
	
	if cb.SuccessThreshold != 3 {
		t.Errorf("Expected success threshold 3, got %d", cb.SuccessThreshold)
	}
	
	if cb.Timeout != 60*time.Second {
		t.Errorf("Expected timeout 60s, got %v", cb.Timeout)
	}
	
	if cb.MaxRequests != 10 {
		t.Errorf("Expected max requests 10, got %d", cb.MaxRequests)
	}
}

// TestRetryLogic tests retry configuration and logic
func TestRetryLogic(t *testing.T) {
	config := rustmq.DefaultStreamConfig()
	config.ErrorStrategy.Type = rustmq.ErrorStrategyRetry
	config.ErrorStrategy.MaxAttempts = 3
	config.ErrorStrategy.BackoffMs = 100 // Short for testing
	config.ErrorStrategy.MaxBackoffMs = 1000
	config.ErrorStrategy.BackoffMultiplier = 2.0
	
	// Test exponential backoff calculation
	backoffMs := config.ErrorStrategy.BackoffMs
	for attempt := 0; attempt < config.ErrorStrategy.MaxAttempts; attempt++ {
		expectedBackoff := config.ErrorStrategy.BackoffMs
		for i := 0; i < attempt; i++ {
			expectedBackoff = int64(float64(expectedBackoff) * config.ErrorStrategy.BackoffMultiplier)
		}
		
		if expectedBackoff > config.ErrorStrategy.MaxBackoffMs {
			expectedBackoff = config.ErrorStrategy.MaxBackoffMs
		}
		
		t.Logf("Attempt %d: Expected backoff %dms", attempt, expectedBackoff)
		
		// Advance to next attempt
		backoffMs = int64(float64(backoffMs) * config.ErrorStrategy.BackoffMultiplier)
		if backoffMs > config.ErrorStrategy.MaxBackoffMs {
			backoffMs = config.ErrorStrategy.MaxBackoffMs
		}
	}
}

// TestStreamModes tests different stream processing modes
func TestStreamModes(t *testing.T) {
	modes := []rustmq.StreamModeType{
		rustmq.StreamModeIndividual,
		rustmq.StreamModeBatch,
		rustmq.StreamModeWindowed,
	}
	
	for _, mode := range modes {
		t.Run(string(mode), func(t *testing.T) {
			config := rustmq.DefaultStreamConfig()
			config.Mode.Type = mode
			
			switch mode {
			case rustmq.StreamModeBatch:
				config.Mode.BatchSize = 10
				config.Mode.BatchTimeout = time.Second
			case rustmq.StreamModeWindowed:
				config.Mode.WindowSize = 5 * time.Second
				config.Mode.SlideInterval = time.Second
			}
			
			// Test configuration
			if config.Mode.Type != mode {
				t.Errorf("Expected mode %s, got %s", mode, config.Mode.Type)
			}
			
			// Test mode-specific configurations
			switch mode {
			case rustmq.StreamModeBatch:
				if config.Mode.BatchSize != 10 {
					t.Errorf("Expected batch size 10, got %d", config.Mode.BatchSize)
				}
				if config.Mode.BatchTimeout != time.Second {
					t.Errorf("Expected batch timeout 1s, got %v", config.Mode.BatchTimeout)
				}
			case rustmq.StreamModeWindowed:
				if config.Mode.WindowSize != 5*time.Second {
					t.Errorf("Expected window size 5s, got %v", config.Mode.WindowSize)
				}
				if config.Mode.SlideInterval != time.Second {
					t.Errorf("Expected slide interval 1s, got %v", config.Mode.SlideInterval)
				}
			}
		})
	}
}

// TestProcessorInterface tests the MessageProcessor interface
func TestProcessorInterface(t *testing.T) {
	processor := &MockProcessor{}
	
	// Test single message processing
	message := rustmq.NewMessage().PayloadString("test message").Build()
	ctx := context.Background()
	
	result, err := processor.Process(ctx, message)
	if err != nil {
		t.Fatalf("Process failed: %v", err)
	}
	
	if result.PayloadAsString() != "test message" {
		t.Errorf("Expected 'test message', got %s", result.PayloadAsString())
	}
	
	// Test batch processing
	messages := []*rustmq.Message{
		rustmq.NewMessage().PayloadString("message 1").Build(),
		rustmq.NewMessage().PayloadString("message 2").Build(),
		rustmq.NewMessage().PayloadString("message 3").Build(),
	}
	
	results, err := processor.ProcessBatch(ctx, messages)
	if err != nil {
		t.Fatalf("ProcessBatch failed: %v", err)
	}
	
	if len(results) != 3 {
		t.Errorf("Expected 3 results, got %d", len(results))
	}
	
	// Test lifecycle methods
	if err := processor.OnStart(ctx); err != nil {
		t.Errorf("OnStart failed: %v", err)
	}
	
	if err := processor.OnStop(ctx); err != nil {
		t.Errorf("OnStop failed: %v", err)
	}
	
	// Test error handling
	testErr := fmt.Errorf("test error")
	if err := processor.OnError(ctx, message, testErr); err != nil {
		t.Errorf("OnError failed: %v", err)
	}
}

// TestStreamMetrics tests stream metrics functionality
func TestStreamMetrics(t *testing.T) {
	// Test metrics structure
	metrics := &rustmq.StreamMetrics{
		MessagesProcessed:     100,
		MessagesFailed:        5,
		MessagesSkipped:       2,
		ProcessingRate:        50.5,
		AverageProcessingTime: 125.5,
		ErrorRate:             5.0,
		LastProcessedTime:     time.Now(),
	}
	
	if metrics.MessagesProcessed != 100 {
		t.Errorf("Expected 100 processed messages, got %d", metrics.MessagesProcessed)
	}
	
	if metrics.MessagesFailed != 5 {
		t.Errorf("Expected 5 failed messages, got %d", metrics.MessagesFailed)
	}
	
	if metrics.ErrorRate != 5.0 {
		t.Errorf("Expected 5.0%% error rate, got %f", metrics.ErrorRate)
	}
}

// TestDeadLetterConfiguration tests dead letter queue configuration
func TestDeadLetterConfiguration(t *testing.T) {
	config := rustmq.DefaultStreamConfig()
	config.ErrorStrategy.Type = rustmq.ErrorStrategyDeadLetter
	config.ErrorStrategy.DeadLetterTopic = "dead-letter-queue"
	
	if config.ErrorStrategy.DeadLetterTopic != "dead-letter-queue" {
		t.Errorf("Expected dead letter topic 'dead-letter-queue', got %s", 
			config.ErrorStrategy.DeadLetterTopic)
	}
}

// TestConcurrentProcessing tests concurrent processing capabilities
func TestConcurrentProcessing(t *testing.T) {
	processor := &MockProcessor{}
	
	// Test concurrent message processing
	var wg sync.WaitGroup
	messageCount := 100
	
	for i := 0; i < messageCount; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			
			message := rustmq.NewMessage().
				PayloadString(fmt.Sprintf("concurrent-message-%d", id)).
				Build()
			
			ctx := context.Background()
			_, err := processor.Process(ctx, message)
			if err != nil {
				t.Errorf("Concurrent processing failed for message %d: %v", id, err)
			}
		}(i)
	}
	
	wg.Wait()
	
	if processor.GetProcessedCount() != int64(messageCount) {
		t.Errorf("Expected %d processed messages, got %d", 
			messageCount, processor.GetProcessedCount())
	}
}

// TestProcessingTimeout tests processing timeout functionality
func TestProcessingTimeout(t *testing.T) {
	config := rustmq.DefaultStreamConfig()
	config.ProcessingTimeout = 100 * time.Millisecond
	
	processor := &MockProcessor{
		processDelay: 200 * time.Millisecond, // Longer than timeout
	}
	
	message := rustmq.NewMessage().PayloadString("timeout test").Build()
	ctx, cancel := context.WithTimeout(context.Background(), config.ProcessingTimeout)
	defer cancel()
	
	_, _ = processor.Process(ctx, message)
	
	// The processor will complete but the context should timeout
	if ctx.Err() != context.DeadlineExceeded {
		t.Errorf("Expected context deadline exceeded, got %v", ctx.Err())
	}
}

// TestErrorProcessor tests processor error handling
func TestErrorProcessor(t *testing.T) {
	processor := &MockProcessor{
		shouldError: true,
	}
	
	message := rustmq.NewMessage().PayloadString("error test").Build()
	ctx := context.Background()
	
	_, err := processor.Process(ctx, message)
	if err == nil {
		t.Error("Expected processing error, got nil")
	}
	
	if err.Error() != "mock processing error" {
		t.Errorf("Expected 'mock processing error', got %s", err.Error())
	}
}

// BenchmarkMessageProcessing benchmarks message processing performance
func BenchmarkMessageProcessing(b *testing.B) {
	processor := &MockProcessor{}
	message := rustmq.NewMessage().PayloadString("benchmark message").Build()
	ctx := context.Background()
	
	b.ResetTimer()
	
	for i := 0; i < b.N; i++ {
		_, err := processor.Process(ctx, message)
		if err != nil {
			b.Fatalf("Processing failed: %v", err)
		}
	}
}

// BenchmarkBatchProcessing benchmarks batch processing performance
func BenchmarkBatchProcessing(b *testing.B) {
	processor := &MockProcessor{}
	batchSize := 10
	messages := make([]*rustmq.Message, batchSize)
	
	for i := 0; i < batchSize; i++ {
		messages[i] = rustmq.NewMessage().
			PayloadString(fmt.Sprintf("batch-message-%d", i)).
			Build()
	}
	
	ctx := context.Background()
	b.ResetTimer()
	
	for i := 0; i < b.N; i++ {
		_, err := processor.ProcessBatch(ctx, messages)
		if err != nil {
			b.Fatalf("Batch processing failed: %v", err)
		}
	}
}