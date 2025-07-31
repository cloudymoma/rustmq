package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// UppercaseProcessor transforms messages to uppercase
type UppercaseProcessor struct{}

// Process implements the MessageProcessor interface
func (p *UppercaseProcessor) Process(ctx context.Context, message *rustmq.Message) (*rustmq.Message, error) {
	// Transform payload to uppercase
	uppercasePayload := strings.ToUpper(message.PayloadAsString())

	// Create transformed message
	transformed := rustmq.NewMessage().
		Topic("processed-topic").
		PayloadString(uppercasePayload).
		Header("processor", "uppercase").
		Header("original-topic", message.Topic).
		Header("processed-at", time.Now().Format(time.RFC3339)).
		Header("original-id", message.ID).
		Build()

	return transformed, nil
}

// ProcessBatch processes multiple messages at once
func (p *UppercaseProcessor) ProcessBatch(ctx context.Context, messages []*rustmq.Message) ([]*rustmq.Message, error) {
	results := make([]*rustmq.Message, len(messages))
	
	for i, message := range messages {
		result, err := p.Process(ctx, message)
		if err != nil {
			return results[:i], err
		}
		results[i] = result
	}
	
	return results, nil
}

// OnStart is called when the stream starts
func (p *UppercaseProcessor) OnStart(ctx context.Context) error {
	fmt.Println("UppercaseProcessor started")
	return nil
}

// OnStop is called when the stream stops
func (p *UppercaseProcessor) OnStop(ctx context.Context) error {
	fmt.Println("UppercaseProcessor stopped")
	return nil
}

// OnError is called when processing fails
func (p *UppercaseProcessor) OnError(ctx context.Context, message *rustmq.Message, err error) error {
	fmt.Printf("Processing error for message %s: %v\n", message.ID, err)
	return nil
}

func main() {
	// Create client configuration
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092"}
	config.ClientID = "stream-processor-go"

	// Create client
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	fmt.Println("Connected to RustMQ cluster")

	// Configure stream processing
	streamConfig := &rustmq.StreamConfig{
		InputTopics:       []string{"input-topic"},
		OutputTopic:       "output-topic",
		ConsumerGroup:     "stream-processors-go",
		Parallelism:       4,
		ProcessingTimeout: 30 * time.Second,
		ExactlyOnce:       false,
		MaxInFlight:       100,
		ErrorStrategy: rustmq.ErrorStrategy{
			Type:        rustmq.ErrorStrategyRetry,
			MaxAttempts: 3,
			BackoffMs:   1000,
		},
		Mode: rustmq.StreamMode{
			Type: rustmq.StreamModeIndividual,
		},
	}

	// Create message stream with custom processor
	stream, err := client.CreateStream(streamConfig)
	if err != nil {
		log.Fatalf("Failed to create stream: %v", err)
	}

	// Set custom processor
	stream = stream.WithProcessor(&UppercaseProcessor{})

	fmt.Println("Created stream processor: input-topic -> output-topic")

	// Start processing
	ctx := context.Background()
	if err := stream.Start(ctx); err != nil {
		log.Fatalf("Failed to start stream: %v", err)
	}

	fmt.Println("Stream processing started")

	// Monitor metrics in a separate goroutine
	go func() {
		ticker := time.NewTicker(10 * time.Second)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				if !stream.IsRunning() {
					return
				}
				
				metrics := stream.Metrics()
				fmt.Printf("Stream metrics - Processed: %d, Failed: %d, Skipped: %d, Rate: %.2f msg/s, Avg Time: %.2f ms\n", 
					metrics.MessagesProcessed, 
					metrics.MessagesFailed, 
					metrics.MessagesSkipped,
					metrics.ProcessingRate,
					metrics.AverageProcessingTime)
			}
		}
	}()

	// Wait for shutdown signal
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	<-sigChan
	fmt.Println("\nReceived shutdown signal, stopping stream processor...")

	// Stop processing
	stopCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := stream.Stop(stopCtx); err != nil {
		fmt.Printf("Error stopping stream: %v\n", err)
	} else {
		fmt.Println("Stream processing stopped")
	}

	// Show final metrics
	finalMetrics := stream.Metrics()
	fmt.Printf("Final stream metrics:\n")
	fmt.Printf("  Messages processed: %d\n", finalMetrics.MessagesProcessed)
	fmt.Printf("  Messages failed: %d\n", finalMetrics.MessagesFailed)
	fmt.Printf("  Messages skipped: %d\n", finalMetrics.MessagesSkipped)
	fmt.Printf("  Error rate: %.2f%%\n", finalMetrics.ErrorRate)
	fmt.Printf("  Average processing time: %.2f ms\n", finalMetrics.AverageProcessingTime)
	if !finalMetrics.LastProcessedTime.IsZero() {
		fmt.Printf("  Last processed: %v\n", finalMetrics.LastProcessedTime.Format(time.RFC3339))
	}

	fmt.Println("Client closed")
}