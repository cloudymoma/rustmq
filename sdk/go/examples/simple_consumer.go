package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Create client configuration
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092"}
	config.ClientID = "simple-consumer-go"

	// Create client
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	fmt.Println("Connected to RustMQ cluster")

	// Create consumer
	consumer, err := client.CreateConsumer("example-topic", "example-group")
	if err != nil {
		log.Fatalf("Failed to create consumer: %v", err)
	}
	defer consumer.Close()

	fmt.Println("Created consumer for topic: example-topic, group: example-group")

	// Handle Ctrl+C gracefully
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	messageCount := 0
	done := make(chan bool)

	// Start message consumption in a goroutine
	go func() {
		defer func() { done <- true }()

		for {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			
			message, err := consumer.Receive(ctx)
			cancel()

			if err != nil {
				if err == context.DeadlineExceeded {
					// Timeout is normal, continue
					continue
				}
				if err.Error() == "consumer is closed" {
					fmt.Println("Consumer closed, stopping message processing")
					break
				}
				fmt.Printf("Error receiving message: %v\n", err)
				time.Sleep(1 * time.Second)
				continue
			}

			if message == nil {
				fmt.Println("Consumer closed")
				break
			}

			messageCount++
			
			fmt.Printf("Received message #%d: id=%s, offset=%d, partition=%d\n", 
				messageCount, message.Message.ID, message.Message.Offset, message.Partition())
			
			fmt.Printf("  Payload: %s\n", message.Message.PayloadAsString())
			
			// Print headers
			for key, value := range message.Message.Headers {
				fmt.Printf("  Header %s: %s\n", key, value)
			}
			
			// Acknowledge the message
			if err := message.Ack(); err != nil {
				fmt.Printf("Failed to acknowledge message: %v\n", err)
			} else {
				fmt.Println("  Message acknowledged")
			}
			
			// Show message age
			fmt.Printf("  Message age: %v\n", message.Message.Age())
		}
	}()

	// Wait for shutdown signal
	<-sigChan
	fmt.Println("\nReceived shutdown signal, closing consumer...")

	// Close consumer (this will break the receive loop)
	if err := consumer.Close(); err != nil {
		fmt.Printf("Error closing consumer: %v\n", err)
	}

	// Wait for message processing to finish
	select {
	case <-done:
		fmt.Println("Message processing stopped")
	case <-time.After(5 * time.Second):
		fmt.Println("Timeout waiting for message processing to stop")
	}

	// Show consumer metrics
	metrics := consumer.Metrics()
	fmt.Printf("Consumer metrics: received=%d, processed=%d, failed=%d, lag=%d\n", 
		metrics.MessagesReceived, metrics.MessagesProcessed, 
		metrics.MessagesFailed, metrics.Lag)

	fmt.Printf("Consumer and client closed. Total messages processed: %d\n", messageCount)
}