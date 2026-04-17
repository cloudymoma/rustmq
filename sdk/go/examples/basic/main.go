package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	// Initialize a new RustMQ client
	client, err := rustmq.NewClient(&rustmq.ClientConfig{
		Brokers: []string{"localhost:9092"},
		Auth:    &rustmq.AuthConfig{Method: rustmq.AuthNone},
	})
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	// Create a producer
	producer, err := client.CreateProducer("my-topic", &rustmq.ProducerConfig{})
	if err != nil {
		log.Fatalf("Failed to create producer: %v", err)
	}

	// Send a message
	msg := rustmq.NewMessage().
		Topic("my-topic").
		KeyString("user-123").
		PayloadString("hello world").
		Header("source", "go-sdk").
		Build()

	fmt.Println("Sending message...")
	_, err = producer.Send(context.Background(), msg)
	if err != nil {
		log.Fatalf("Failed to send message: %v", err)
	}

	// Create a consumer
	consumer, err := client.CreateConsumer("my-topic", "my-consumer-group", &rustmq.ConsumerConfig{})
	if err != nil {
		log.Fatalf("Failed to create consumer: %v", err)
	}
	defer consumer.Close()

	// Receive the message
	fmt.Println("Waiting for message...")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	receivedMsg, err := consumer.Receive(ctx)
	if err != nil {
		log.Fatalf("Failed to receive message: %v", err)
	}

	fmt.Printf("Received: Key=%s, Payload=%s\n", receivedMsg.Message.KeyAsString(), receivedMsg.Message.PayloadAsString())

	// Acknowledge the message
	if err := receivedMsg.Ack(); err != nil {
		log.Fatalf("Failed to acknowledge message: %v", err)
	}
	fmt.Println("Message acknowledged successfully")
}
