package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// AdvancedProcessor demonstrates comprehensive stream processing features
type AdvancedProcessor struct {
	processedCount int64
}

// Process handles individual message processing
func (ap *AdvancedProcessor) Process(ctx context.Context, message *rustmq.Message) (*rustmq.Message, error) {
	ap.processedCount++
	
	// Add processing timestamp
	processed := message.Clone()
	if processed.Headers == nil {
		processed.Headers = make(map[string]string)
	}
	processed.Headers["processed_at"] = time.Now().Format(time.RFC3339)
	processed.Headers["processor"] = "advanced-processor"
	
	// Simulate processing work
	time.Sleep(10 * time.Millisecond)
	
	log.Printf("Processed individual message: %s", message.ID)
	return processed, nil
}

// ProcessBatch handles batch processing with optimizations
func (ap *AdvancedProcessor) ProcessBatch(ctx context.Context, messages []*rustmq.Message) ([]*rustmq.Message, error) {
	ap.processedCount += int64(len(messages))
	
	log.Printf("Processing batch of %d messages", len(messages))
	
	results := make([]*rustmq.Message, len(messages))
	for i, message := range messages {
		processed := message.Clone()
		if processed.Headers == nil {
			processed.Headers = make(map[string]string)
		}
		processed.Headers["batch_processed_at"] = time.Now().Format(time.RFC3339)
		processed.Headers["batch_size"] = fmt.Sprintf("%d", len(messages))
		processed.Headers["batch_index"] = fmt.Sprintf("%d", i)
		
		results[i] = processed
	}
	
	// Simulate batch processing work (more efficient than individual)
	time.Sleep(time.Duration(len(messages)) * 2 * time.Millisecond)
	
	return results, nil
}

// OnStart is called when the stream starts
func (ap *AdvancedProcessor) OnStart(ctx context.Context) error {
	log.Println("Advanced processor starting...")
	ap.processedCount = 0
	return nil
}

// OnStop is called when the stream stops
func (ap *AdvancedProcessor) OnStop(ctx context.Context) error {
	log.Printf("Advanced processor stopping... Processed %d messages total", ap.processedCount)
	return nil
}

// OnError handles processing errors
func (ap *AdvancedProcessor) OnError(ctx context.Context, message *rustmq.Message, err error) error {
	log.Printf("Error processing message %s: %v", message.ID, err)
	
	// Add error information to message headers for debugging
	if message.Headers == nil {
		message.Headers = make(map[string]string)
	}
	message.Headers["error"] = err.Error()
	message.Headers["error_timestamp"] = time.Now().Format(time.RFC3339)
	
	return nil
}

func main() {
	// Example 1: Batch Processing with Error Handling
	fmt.Println("=== Example 1: Batch Processing ===")
	demonstrateBatchProcessing()
	
	// Example 2: Windowed Processing
	fmt.Println("\n=== Example 2: Windowed Processing ===")
	demonstrateWindowedProcessing()
	
	// Example 3: Error Handling Strategies
	fmt.Println("\n=== Example 3: Error Handling Strategies ===")
	demonstrateErrorHandling()
	
	// Example 4: Circuit Breaker Pattern
	fmt.Println("\n=== Example 4: Circuit Breaker Pattern ===")
	demonstrateCircuitBreaker()
}

func demonstrateBatchProcessing() {
	// Configure batch processing
	config := rustmq.DefaultStreamConfig()
	config.Mode.Type = rustmq.StreamModeBatch
	config.Mode.BatchSize = 5           // Process in batches of 5
	config.Mode.BatchTimeout = 2 * time.Second // Or every 2 seconds
	config.InputTopics = []string{"input-batch"}
	config.OutputTopic = "output-batch"
	config.ProcessingTimeout = 30 * time.Second
	config.MaxInFlight = 20
	
	// Configure retry strategy
	config.ErrorStrategy.Type = rustmq.ErrorStrategyRetry
	config.ErrorStrategy.MaxAttempts = 3
	config.ErrorStrategy.BackoffMs = 1000
	config.ErrorStrategy.MaxBackoffMs = 10000
	config.ErrorStrategy.BackoffMultiplier = 2.0
	
	fmt.Printf("Batch Configuration:\n")
	fmt.Printf("- Batch Size: %d\n", config.Mode.BatchSize)
	fmt.Printf("- Batch Timeout: %v\n", config.Mode.BatchTimeout)
	fmt.Printf("- Max Attempts: %d\n", config.ErrorStrategy.MaxAttempts)
	fmt.Printf("- Backoff: %dms (max: %dms)\n", 
		config.ErrorStrategy.BackoffMs, config.ErrorStrategy.MaxBackoffMs)
}

func demonstrateWindowedProcessing() {
	// Configure windowed processing
	config := rustmq.DefaultStreamConfig()
	config.Mode.Type = rustmq.StreamModeWindowed
	config.Mode.WindowSize = 10 * time.Second    // 10-second windows
	config.Mode.SlideInterval = 5 * time.Second  // Slide every 5 seconds (50% overlap)
	config.InputTopics = []string{"input-windowed"}
	config.OutputTopic = "output-aggregated"
	
	fmt.Printf("Windowed Configuration:\n")
	fmt.Printf("- Window Size: %v\n", config.Mode.WindowSize)
	fmt.Printf("- Slide Interval: %v\n", config.Mode.SlideInterval)
	fmt.Printf("- Window Type: %s\n", getWindowType(config.Mode.SlideInterval))
}

func demonstrateErrorHandling() {
	strategies := []struct {
		name     string
		strategy rustmq.ErrorStrategyType
		config   func(*rustmq.StreamConfig)
	}{
		{
			name:     "Skip Failed Messages",
			strategy: rustmq.ErrorStrategySkip,
			config:   func(c *rustmq.StreamConfig) {},
		},
		{
			name:     "Retry with Exponential Backoff",
			strategy: rustmq.ErrorStrategyRetry,
			config: func(c *rustmq.StreamConfig) {
				c.ErrorStrategy.MaxAttempts = 5
				c.ErrorStrategy.BackoffMs = 500
				c.ErrorStrategy.MaxBackoffMs = 30000
				c.ErrorStrategy.BackoffMultiplier = 2.0
			},
		},
		{
			name:     "Dead Letter Queue",
			strategy: rustmq.ErrorStrategyDeadLetter,
			config: func(c *rustmq.StreamConfig) {
				c.ErrorStrategy.DeadLetterTopic = "failed-messages"
			},
		},
		{
			name:     "Stop on Error",
			strategy: rustmq.ErrorStrategyStop,
			config:   func(c *rustmq.StreamConfig) {},
		},
	}
	
	for _, s := range strategies {
		config := rustmq.DefaultStreamConfig()
		config.ErrorStrategy.Type = s.strategy
		s.config(config)
		
		fmt.Printf("Strategy: %s\n", s.name)
		fmt.Printf("- Type: %s\n", config.ErrorStrategy.Type)
		if config.ErrorStrategy.MaxAttempts > 0 {
			fmt.Printf("- Max Attempts: %d\n", config.ErrorStrategy.MaxAttempts)
		}
		if config.ErrorStrategy.DeadLetterTopic != "" {
			fmt.Printf("- Dead Letter Topic: %s\n", config.ErrorStrategy.DeadLetterTopic)
		}
		fmt.Println()
	}
}

func demonstrateCircuitBreaker() {
	config := rustmq.DefaultStreamConfig()
	config.ErrorStrategy.CircuitBreaker = &rustmq.CircuitBreakerConfig{
		FailureThreshold: 10,                // Open after 10 failures
		SuccessThreshold: 5,                 // Close after 5 successes
		Timeout:          60 * time.Second,  // Try again after 60 seconds
		MaxRequests:      20,                // Allow 20 requests in half-open state
	}
	
	fmt.Printf("Circuit Breaker Configuration:\n")
	fmt.Printf("- Failure Threshold: %d\n", config.ErrorStrategy.CircuitBreaker.FailureThreshold)
	fmt.Printf("- Success Threshold: %d\n", config.ErrorStrategy.CircuitBreaker.SuccessThreshold)
	fmt.Printf("- Timeout: %v\n", config.ErrorStrategy.CircuitBreaker.Timeout)
	fmt.Printf("- Max Requests (Half-Open): %d\n", config.ErrorStrategy.CircuitBreaker.MaxRequests)
	
	fmt.Println("\nCircuit Breaker States:")
	fmt.Println("- CLOSED: Normal operation, requests pass through")
	fmt.Println("- OPEN: Failures detected, requests are blocked")
	fmt.Println("- HALF-OPEN: Testing if service has recovered")
}

func getWindowType(slideInterval time.Duration) string {
	if slideInterval == 0 {
		return "Tumbling (non-overlapping)"
	}
	return "Sliding (overlapping)"
}

// Example usage of the stream processor
func exampleStreamUsage() {
	// This would be used with a real RustMQ client
	/*
	client, err := rustmq.NewClient(&rustmq.ClientConfig{
		Brokers: []string{"localhost:9092"},
	})
	if err != nil {
		log.Fatal(err)
	}
	defer client.Close()
	
	// Create stream with batch processing
	config := rustmq.DefaultStreamConfig()
	config.Mode.Type = rustmq.StreamModeBatch
	config.Mode.BatchSize = 10
	config.Mode.BatchTimeout = 5 * time.Second
	config.ErrorStrategy.Type = rustmq.ErrorStrategyRetry
	config.ErrorStrategy.MaxAttempts = 3
	config.ErrorStrategy.DeadLetterTopic = "failed-messages"
	
	stream, err := client.CreateStream(config)
	if err != nil {
		log.Fatal(err)
	}
	
	// Set custom processor
	processor := &AdvancedProcessor{}
	stream.WithProcessor(processor)
	
	// Start processing
	ctx := context.Background()
	if err := stream.Start(ctx); err != nil {
		log.Fatal(err)
	}
	
	// Let it run for a while
	time.Sleep(30 * time.Second)
	
	// Check metrics
	metrics := stream.Metrics()
	log.Printf("Processed: %d, Failed: %d, Error Rate: %.2f%%",
		metrics.MessagesProcessed, metrics.MessagesFailed, metrics.ErrorRate)
	
	// Stop gracefully
	if err := stream.Stop(ctx); err != nil {
		log.Printf("Error stopping stream: %v", err)
	}
	*/
}