package rustmq

import (
	"encoding/json"
	"time"

	"github.com/google/uuid"
)

// Message represents a message in the RustMQ system
type Message struct {
	ID          string            `json:"id"`
	Topic       string            `json:"topic"`
	Partition   uint32            `json:"partition"`
	Offset      uint64            `json:"offset"`
	Key         []byte            `json:"key,omitempty"`
	Payload     []byte            `json:"payload"`
	Headers     map[string]string `json:"headers,omitempty"`
	Timestamp   time.Time         `json:"timestamp"`
	ProducerID  string            `json:"producer_id,omitempty"`
	Sequence    *uint64           `json:"sequence,omitempty"`
	Compression string            `json:"compression,omitempty"`
	Size        int               `json:"size"`
}

// MessageBuilder helps build messages
type MessageBuilder struct {
	message *Message
}

// NewMessage creates a new message builder
func NewMessage() *MessageBuilder {
	return &MessageBuilder{
		message: &Message{
			ID:        uuid.New().String(),
			Headers:   make(map[string]string),
			Timestamp: time.Now(),
		},
	}
}

// Topic sets the topic for the message
func (mb *MessageBuilder) Topic(topic string) *MessageBuilder {
	mb.message.Topic = topic
	return mb
}

// Partition sets the partition for the message
func (mb *MessageBuilder) Partition(partition uint32) *MessageBuilder {
	mb.message.Partition = partition
	return mb
}

// Key sets the key for the message
func (mb *MessageBuilder) Key(key []byte) *MessageBuilder {
	mb.message.Key = make([]byte, len(key))
	copy(mb.message.Key, key)
	return mb
}

// KeyString sets the key for the message from a string
func (mb *MessageBuilder) KeyString(key string) *MessageBuilder {
	mb.message.Key = []byte(key)
	return mb
}

// Payload sets the payload for the message
func (mb *MessageBuilder) Payload(payload []byte) *MessageBuilder {
	mb.message.Payload = make([]byte, len(payload))
	copy(mb.message.Payload, payload)
	mb.message.Size = len(payload)
	return mb
}

// PayloadString sets the payload for the message from a string
func (mb *MessageBuilder) PayloadString(payload string) *MessageBuilder {
	mb.message.Payload = []byte(payload)
	mb.message.Size = len(payload)
	return mb
}

// PayloadJSON sets the payload for the message from a JSON-serializable object
func (mb *MessageBuilder) PayloadJSON(payload interface{}) *MessageBuilder {
	data, err := json.Marshal(payload)
	if err != nil {
		// In a real implementation, we'd handle this error properly
		return mb
	}
	mb.message.Payload = data
	mb.message.Size = len(data)
	mb.message.Headers["content-type"] = "application/json"
	return mb
}

// Header sets a header for the message
func (mb *MessageBuilder) Header(key, value string) *MessageBuilder {
	if mb.message.Headers == nil {
		mb.message.Headers = make(map[string]string)
	}
	mb.message.Headers[key] = value
	return mb
}

// Headers sets multiple headers for the message
func (mb *MessageBuilder) Headers(headers map[string]string) *MessageBuilder {
	if mb.message.Headers == nil {
		mb.message.Headers = make(map[string]string)
	}
	for key, value := range headers {
		mb.message.Headers[key] = value
	}
	return mb
}

// ID sets the ID for the message
func (mb *MessageBuilder) ID(id string) *MessageBuilder {
	mb.message.ID = id
	return mb
}

// Timestamp sets the timestamp for the message
func (mb *MessageBuilder) Timestamp(timestamp time.Time) *MessageBuilder {
	mb.message.Timestamp = timestamp
	return mb
}

// Build returns the built message
func (mb *MessageBuilder) Build() *Message {
	// Calculate total size including headers and key
	totalSize := len(mb.message.Payload)
	if mb.message.Key != nil {
		totalSize += len(mb.message.Key)
	}
	for key, value := range mb.message.Headers {
		totalSize += len(key) + len(value)
	}
	mb.message.Size = totalSize

	return mb.message
}

// Clone creates a copy of the message
func (m *Message) Clone() *Message {
	clone := &Message{
		ID:          m.ID,
		Topic:       m.Topic,
		Partition:   m.Partition,
		Offset:      m.Offset,
		Timestamp:   m.Timestamp,
		ProducerID:  m.ProducerID,
		Compression: m.Compression,
		Size:        m.Size,
	}

	if m.Key != nil {
		clone.Key = make([]byte, len(m.Key))
		copy(clone.Key, m.Key)
	}

	if m.Payload != nil {
		clone.Payload = make([]byte, len(m.Payload))
		copy(clone.Payload, m.Payload)
	}

	if m.Headers != nil {
		clone.Headers = make(map[string]string, len(m.Headers))
		for k, v := range m.Headers {
			clone.Headers[k] = v
		}
	}

	if m.Sequence != nil {
		seq := *m.Sequence
		clone.Sequence = &seq
	}

	return clone
}

// KeyAsString returns the message key as a string
func (m *Message) KeyAsString() string {
	if m.Key == nil {
		return ""
	}
	return string(m.Key)
}

// PayloadAsString returns the message payload as a string
func (m *Message) PayloadAsString() string {
	return string(m.Payload)
}

// PayloadAsJSON unmarshals the payload as JSON into the provided interface
func (m *Message) PayloadAsJSON(v interface{}) error {
	return json.Unmarshal(m.Payload, v)
}

// HasHeader checks if the message has a specific header
func (m *Message) HasHeader(key string) bool {
	_, exists := m.Headers[key]
	return exists
}

// GetHeader gets a header value
func (m *Message) GetHeader(key string) (string, bool) {
	value, exists := m.Headers[key]
	return value, exists
}

// HeaderKeys returns all header keys
func (m *Message) HeaderKeys() []string {
	keys := make([]string, 0, len(m.Headers))
	for key := range m.Headers {
		keys = append(keys, key)
	}
	return keys
}

// Age returns how old the message is
func (m *Message) Age() time.Duration {
	return time.Since(m.Timestamp)
}

// IsCompressed checks if the message is compressed
func (m *Message) IsCompressed() bool {
	return m.Compression != ""
}

// TotalSize returns the total size of the message including all components
func (m *Message) TotalSize() int {
	size := len(m.Payload)
	
	if m.Key != nil {
		size += len(m.Key)
	}
	
	for key, value := range m.Headers {
		size += len(key) + len(value)
	}
	
	size += len(m.ID) + len(m.Topic) + len(m.ProducerID)
	
	return size
}

// ToJSON serializes the message to JSON
func (m *Message) ToJSON() ([]byte, error) {
	return json.Marshal(m)
}

// FromJSON deserializes a message from JSON
func FromJSON(data []byte) (*Message, error) {
	var message Message
	err := json.Unmarshal(data, &message)
	if err != nil {
		return nil, err
	}
	return &message, nil
}

// MessageMetadata represents lightweight message metadata
type MessageMetadata struct {
	ID        string            `json:"id"`
	Topic     string            `json:"topic"`
	Partition uint32            `json:"partition"`
	Offset    uint64            `json:"offset"`
	Timestamp time.Time         `json:"timestamp"`
	Size      int               `json:"size"`
	Headers   map[string]string `json:"headers,omitempty"`
}

// Metadata extracts metadata from the message
func (m *Message) Metadata() *MessageMetadata {
	headers := make(map[string]string)
	for k, v := range m.Headers {
		headers[k] = v
	}
	
	return &MessageMetadata{
		ID:        m.ID,
		Topic:     m.Topic,
		Partition: m.Partition,
		Offset:    m.Offset,
		Timestamp: m.Timestamp,
		Size:      m.Size,
		Headers:   headers,
	}
}

// MessageBatch represents a batch of messages for efficient processing
type MessageBatch struct {
	Messages  []*Message `json:"messages"`
	BatchID   string     `json:"batch_id"`
	TotalSize int        `json:"total_size"`
	CreatedAt time.Time  `json:"created_at"`
}

// NewMessageBatch creates a new message batch
func NewMessageBatch(messages []*Message) *MessageBatch {
	totalSize := 0
	for _, msg := range messages {
		totalSize += msg.TotalSize()
	}

	return &MessageBatch{
		Messages:  messages,
		BatchID:   uuid.New().String(),
		TotalSize: totalSize,
		CreatedAt: time.Now(),
	}
}

// Len returns the number of messages in the batch
func (mb *MessageBatch) Len() int {
	return len(mb.Messages)
}

// IsEmpty checks if the batch is empty
func (mb *MessageBatch) IsEmpty() bool {
	return len(mb.Messages) == 0
}

// SplitBySize splits the batch into smaller batches by size
func (mb *MessageBatch) SplitBySize(maxSize int) []*MessageBatch {
	if mb.TotalSize <= maxSize {
		return []*MessageBatch{mb}
	}

	var batches []*MessageBatch
	var currentBatch []*Message
	currentSize := 0

	for _, message := range mb.Messages {
		messageSize := message.TotalSize()
		
		if currentSize+messageSize > maxSize && len(currentBatch) > 0 {
			batches = append(batches, NewMessageBatch(currentBatch))
			currentBatch = []*Message{}
			currentSize = 0
		}
		
		currentBatch = append(currentBatch, message)
		currentSize += messageSize
	}

	if len(currentBatch) > 0 {
		batches = append(batches, NewMessageBatch(currentBatch))
	}

	return batches
}

// SplitByCount splits the batch into smaller batches by message count
func (mb *MessageBatch) SplitByCount(maxCount int) []*MessageBatch {
	if len(mb.Messages) <= maxCount {
		return []*MessageBatch{mb}
	}

	var batches []*MessageBatch
	for i := 0; i < len(mb.Messages); i += maxCount {
		end := i + maxCount
		if end > len(mb.Messages) {
			end = len(mb.Messages)
		}
		
		batchMessages := mb.Messages[i:end]
		batches = append(batches, NewMessageBatch(batchMessages))
	}

	return batches
}