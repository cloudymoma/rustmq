package tests

import (
	"testing"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
	"github.com/stretchr/testify/assert"
)

func TestMessageBuilder(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("test-topic").
		PayloadString("Hello, World!").
		Header("key", "value").
		Header("timestamp", time.Now().Format(time.RFC3339)).
		Build()

	assert.Equal(t, "test-topic", message.Topic)
	assert.Equal(t, "Hello, World!", message.PayloadAsString())
	assert.True(t, message.HasHeader("key"))
	assert.Equal(t, "value", message.Headers["key"])
	assert.NotEmpty(t, message.ID)
	assert.False(t, message.Timestamp.IsZero())
}

func TestMessageBuilderWithKey(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("test-topic").
		KeyString("message-key").
		PayloadString("Hello, World!").
		Build()

	assert.Equal(t, "message-key", message.KeyAsString())
	assert.NotNil(t, message.Key)
	assert.Equal(t, []byte("message-key"), message.Key)
}

func TestMessageBuilderWithJSONPayload(t *testing.T) {
	testData := map[string]interface{}{
		"name":    "John Doe",
		"age":     30,
		"active":  true,
		"scores":  []int{85, 90, 92},
	}

	message := rustmq.NewMessage().
		Topic("user-events").
		PayloadJSON(testData).
		Build()

	assert.True(t, message.HasHeader("content-type"))
	assert.Equal(t, "application/json", message.Headers["content-type"])

	var decoded map[string]interface{}
	err := message.PayloadAsJSON(&decoded)
	assert.NoError(t, err)
	
	assert.Equal(t, "John Doe", decoded["name"])
	assert.Equal(t, float64(30), decoded["age"]) // JSON numbers are float64
	assert.True(t, decoded["active"].(bool))
}

func TestMessageClone(t *testing.T) {
	original := rustmq.NewMessage().
		Topic("original-topic").
		KeyString("original-key").
		PayloadString("Original payload").
		Header("original", "true").
		Build()

	clone := original.Clone()

	// Verify they are separate instances
	assert.NotSame(t, original, clone)
	assert.NotSame(t, original.Key, clone.Key)
	assert.NotSame(t, original.Payload, clone.Payload)
	assert.NotSame(t, original.Headers, clone.Headers)

	// Verify content is the same
	assert.Equal(t, original.ID, clone.ID)
	assert.Equal(t, original.Topic, clone.Topic)
	assert.Equal(t, original.KeyAsString(), clone.KeyAsString())
	assert.Equal(t, original.PayloadAsString(), clone.PayloadAsString())
	assert.Equal(t, original.Headers, clone.Headers)

	// Modify clone and verify original is unchanged
	clone.Headers["modified"] = "true"
	assert.False(t, original.HasHeader("modified"))
}

func TestMessageSerialization(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("serialization-test").
		KeyString("test-key").
		PayloadString("Test payload").
		Header("test", "true").
		Build()

	// Serialize to JSON
	jsonData, err := message.ToJSON()
	assert.NoError(t, err)
	assert.NotEmpty(t, jsonData)

	// Deserialize from JSON
	deserialized, err := rustmq.FromJSON(jsonData)
	assert.NoError(t, err)

	assert.Equal(t, message.ID, deserialized.ID)
	assert.Equal(t, message.Topic, deserialized.Topic)
	assert.Equal(t, message.KeyAsString(), deserialized.KeyAsString())
	assert.Equal(t, message.PayloadAsString(), deserialized.PayloadAsString())
	assert.Equal(t, message.Headers, deserialized.Headers)
}

func TestMessageMetadata(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("metadata-test").
		PayloadString("Test payload for metadata").
		Header("important", "true").
		Build()

	metadata := message.Metadata()

	assert.Equal(t, message.ID, metadata.ID)
	assert.Equal(t, message.Topic, metadata.Topic)
	assert.Equal(t, message.Partition, metadata.Partition)
	assert.Equal(t, message.Offset, metadata.Offset)
	assert.Equal(t, message.Timestamp, metadata.Timestamp)
	assert.Equal(t, message.Size, metadata.Size)
	assert.Equal(t, message.Headers, metadata.Headers)
}

func TestMessageAge(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("age-test").
		PayloadString("Test payload").
		Timestamp(time.Now().Add(-5 * time.Second)).
		Build()

	age := message.Age()
	assert.True(t, age >= 5*time.Second)
	assert.True(t, age < 6*time.Second) // Allow some margin
}

func TestMessageTotalSize(t *testing.T) {
	message := rustmq.NewMessage().
		Topic("size-test").
		KeyString("test-key").
		PayloadString("Test payload").
		Header("header1", "value1").
		Header("header2", "value2").
		Build()

	totalSize := message.TotalSize()
	
	expectedSize := len("Test payload") + // payload
		len("test-key") + // key
		len("header1") + len("value1") + // header 1
		len("header2") + len("value2") + // header 2
		len(message.ID) + // ID
		len("size-test") + // topic
		len(message.ProducerID) // producer ID

	assert.Equal(t, expectedSize, totalSize)
}

func TestMessageBatch(t *testing.T) {
	messages := []*rustmq.Message{
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 1").Build(),
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 2").Build(),
		rustmq.NewMessage().Topic("batch-test").PayloadString("Message 3").Build(),
	}

	batch := rustmq.NewMessageBatch(messages)

	assert.Equal(t, 3, batch.Len())
	assert.False(t, batch.IsEmpty())
	assert.NotEmpty(t, batch.BatchID)
	assert.False(t, batch.CreatedAt.IsZero())
	assert.True(t, batch.TotalSize > 0)
}

func TestMessageBatchSplitBySize(t *testing.T) {
	// Create messages with known sizes
	messages := []*rustmq.Message{
		rustmq.NewMessage().Topic("split-test").PayloadString("A").Build(), // Small message
		rustmq.NewMessage().Topic("split-test").PayloadString("B").Build(), // Small message
		rustmq.NewMessage().Topic("split-test").PayloadString("C").Build(), // Small message
	}

	batch := rustmq.NewMessageBatch(messages)
	
	// Split by very small size to force multiple batches
	smallBatches := batch.SplitBySize(50) // Assuming each message is larger than 50 bytes
	
	assert.True(t, len(smallBatches) >= 1)
	
	// Verify all messages are preserved
	totalMessages := 0
	for _, smallBatch := range smallBatches {
		totalMessages += smallBatch.Len()
	}
	assert.Equal(t, 3, totalMessages)
}

func TestMessageBatchSplitByCount(t *testing.T) {
	messages := make([]*rustmq.Message, 5)
	for i := range messages {
		messages[i] = rustmq.NewMessage().
			Topic("count-split-test").
			PayloadString("Message").
			Build()
	}

	batch := rustmq.NewMessageBatch(messages)
	smallBatches := batch.SplitByCount(2) // Split into batches of 2

	assert.Equal(t, 3, len(smallBatches)) // 5 messages → 3 batches (2+2+1)
	assert.Equal(t, 2, smallBatches[0].Len())
	assert.Equal(t, 2, smallBatches[1].Len())
	assert.Equal(t, 1, smallBatches[2].Len())
}

func TestMessageValidation(t *testing.T) {
	// Test empty payload
	message := rustmq.NewMessage().
		Topic("validation-test").
		Build()
	
	assert.Empty(t, message.Payload)
	assert.Equal(t, 0, len(message.Payload))

	// Test with payload
	message = rustmq.NewMessage().
		Topic("validation-test").
		PayloadString("Valid payload").
		Build()
	
	assert.NotEmpty(t, message.Payload)
	assert.Equal(t, "Valid payload", message.PayloadAsString())
}