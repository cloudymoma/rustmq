package benchmarks

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func BenchmarkMessageCreation(b *testing.B) {
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		message := rustmq.NewMessage().
			Topic("benchmark-topic").
			PayloadString("benchmark payload data").
			Header("benchmark", "true").
			Build()
		_ = message
	}
}

func BenchmarkMessageSerialization(b *testing.B) {
	message := rustmq.NewMessage().
		Topic("benchmark-topic").
		PayloadString("benchmark payload data").
		Header("benchmark", "true").
		Build()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		_, err := message.ToJSON()
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkMessageDeserialization(b *testing.B) {
	message := rustmq.NewMessage().
		Topic("benchmark-topic").
		PayloadString("benchmark payload data").
		Header("benchmark", "true").
		Build()
	
	jsonData, err := message.ToJSON()
	if err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		_, err := rustmq.FromJSON(jsonData)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkMessageClone(b *testing.B) {
	message := rustmq.NewMessage().
		Topic("benchmark-topic").
		KeyString("benchmark-key").
		PayloadString("benchmark payload data with some length to make it realistic").
		Header("benchmark", "true").
		Header("timestamp", "1234567890").
		Header("correlation-id", "abc-123-def").
		Build()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		clone := message.Clone()
		_ = clone
	}
}

func BenchmarkBatchCreation(b *testing.B) {
	sizes := []int{10, 100, 1000}
	
	for _, size := range sizes {
		b.Run(fmt.Sprintf("size_%d", size), func(b *testing.B) {
			messages := make([]*rustmq.Message, size)
			for i := 0; i < size; i++ {
				messages[i] = rustmq.NewMessage().
					Topic("benchmark-topic").
					PayloadString("benchmark message").
					Build()
			}
			
			b.ResetTimer()
			b.ReportAllocs()
			
			for i := 0; i < b.N; i++ {
				batch := rustmq.NewMessageBatch(messages)
				_ = batch
			}
		})
	}
}

func BenchmarkClientCreation(b *testing.B) {
	config := rustmq.DefaultClientConfig()
	config.Brokers = []string{"localhost:9092"}
	
	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		client, err := rustmq.NewClient(config)
		if err != nil {
			// Expected to fail without broker, but we're measuring creation overhead
			continue
		}
		client.Close()
	}
}

func BenchmarkProducerSendAsync(b *testing.B) {
	config := rustmq.DefaultClientConfig()
	client, err := rustmq.NewClient(config)
	if err != nil {
		b.Skip("Skipping producer benchmark - no broker available")
	}
	defer client.Close()

	producer, err := client.CreateProducer("benchmark-topic")
	if err != nil {
		b.Skip("Producer creation failed")
	}
	defer producer.Close()

	message := rustmq.NewMessage().
		Topic("benchmark-topic").
		PayloadString("benchmark payload").
		Build()

	ctx := context.Background()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		producer.SendAsync(ctx, message, nil)
	}
}

func BenchmarkMessageMetadataExtraction(b *testing.B) {
	message := rustmq.NewMessage().
		Topic("benchmark-topic").
		PayloadString("benchmark payload data with some length to make it realistic").
		Header("benchmark", "true").
		Header("timestamp", "1234567890").
		Header("correlation-id", "abc-123-def").
		Build()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		metadata := message.Metadata()
		_ = metadata
	}
}

func BenchmarkTopicPartitionOperations(b *testing.B) {
	tp1 := rustmq.NewTopicPartition("topic1", 0)
	tp2 := rustmq.NewTopicPartition("topic1", 0)
	tp3 := rustmq.NewTopicPartition("topic2", 1)

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		eq1 := tp1 == tp2
		eq2 := tp1 == tp3
		_ = eq1
		_ = eq2
	}
}

func BenchmarkConfigSerialization(b *testing.B) {
	config := rustmq.DefaultClientConfig()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		// In a real implementation, we'd serialize the config
		// For now, just access its fields
		_ = config.Brokers
		_ = config.ClientID
		_ = config.ConnectTimeout
	}
}

func BenchmarkStreamConfigCreation(b *testing.B) {
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		config := rustmq.DefaultStreamConfig()
		config.InputTopics = []string{"input1", "input2"}
		config.OutputTopic = "output"
		config.Parallelism = 4
		_ = config
	}
}

func BenchmarkMessageAgeCalculation(b *testing.B) {
	message := rustmq.NewMessage().
		Topic("age-test").
		PayloadString("test payload").
		Timestamp(time.Now().Add(-5 * time.Second)).
		Build()

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		age := message.Age()
		_ = age
	}
}

func BenchmarkConsumerLagCalculation(b *testing.B) {
	currentOffsets := []uint64{100, 200, 300, 400, 500}
	logEndOffsets := []uint64{150, 250, 350, 450, 550}

	b.ResetTimer()
	b.ReportAllocs()
	
	for i := 0; i < b.N; i++ {
		for j := 0; j < len(currentOffsets); j++ {
			lag := rustmq.CalculateLag(currentOffsets[j], logEndOffsets[j])
			_ = lag
		}
	}
}