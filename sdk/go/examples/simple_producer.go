package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Create client configuration
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092"}
	config.ClientID = "simple-producer-go"

	// Create client
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	fmt.Println("Connected to RustMQ cluster")

	// Create producer
	producer, err := client.CreateProducer("example-topic")
	if err != nil {
		log.Fatalf("Failed to create producer: %v", err)
	}
	defer producer.Close()

	fmt.Println("Created producer for topic: example-topic")

	// Send some messages
	for i := 0; i < 10; i++ {
		message := rustmq.NewMessage().
			Topic("example-topic").
			PayloadString(fmt.Sprintf("Hello, World! Message #%d", i)).
			Header("message-id", fmt.Sprintf("msg-%d", i)).
			Header("timestamp", time.Now().Format(time.RFC3339)).
			Header("producer", "simple-producer-go").
			Build()

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		
		result, err := producer.Send(ctx, message)
		if err != nil {
			fmt.Printf("Failed to send message %d: %v\n", i, err)
		} else {
			fmt.Printf("Message %d sent successfully: offset=%d, partition=%d\n", 
				i, result.Offset, result.Partition)
		}
		
		cancel()

		// Small delay between messages
		time.Sleep(100 * time.Millisecond)
	}

	// Flush remaining messages
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	if err := producer.Flush(ctx); err != nil {
		fmt.Printf("Failed to flush messages: %v\n", err)
	} else {
		fmt.Println("All messages flushed")
	}

	// Show producer metrics
	metrics := producer.Metrics()
	fmt.Printf("Producer metrics: sent=%d, failed=%d, batches=%d\n", 
		metrics.MessagesSent, metrics.MessagesFailed, metrics.BatchesSent)

	fmt.Println("Producer and client closed")
}