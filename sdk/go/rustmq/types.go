package rustmq

import "time"

// TopicMetadata holds information about a topic
type TopicMetadata struct {
	Name              string                `json:"name"`
	Partitions        []PartitionMetadata   `json:"partitions"`
	ReplicationFactor uint16                `json:"replication_factor"`
	Config            map[string]string     `json:"config"`
	CreatedAt         time.Time             `json:"created_at"`
}

// PartitionMetadata holds information about a partition
type PartitionMetadata struct {
	ID               uint32       `json:"id"`
	Leader           *BrokerInfo  `json:"leader,omitempty"`
	Replicas         []BrokerInfo `json:"replicas"`
	InSyncReplicas   []BrokerInfo `json:"in_sync_replicas"`
	LogStartOffset   uint64       `json:"log_start_offset"`
	LogEndOffset     uint64       `json:"log_end_offset"`
}

// BrokerInfo holds information about a broker
type BrokerInfo struct {
	ID   uint32  `json:"id"`
	Host string  `json:"host"`
	Port uint16  `json:"port"`
	Rack *string `json:"rack,omitempty"`
}

// ConsumerGroupMetadata holds information about a consumer group
type ConsumerGroupMetadata struct {
	GroupID      string               `json:"group_id"`
	State        ConsumerGroupState   `json:"state"`
	Members      []ConsumerMember     `json:"members"`
	Coordinator  BrokerInfo           `json:"coordinator"`
	ProtocolType string               `json:"protocol_type"`
	Protocol     string               `json:"protocol"`
}

// ConsumerGroupState represents consumer group states
type ConsumerGroupState string

const (
	ConsumerGroupUnknown            ConsumerGroupState = "unknown"
	ConsumerGroupPreparingRebalance ConsumerGroupState = "preparing_rebalance"
	ConsumerGroupCompletingRebalance ConsumerGroupState = "completing_rebalance"
	ConsumerGroupStable             ConsumerGroupState = "stable"
	ConsumerGroupDead               ConsumerGroupState = "dead"
	ConsumerGroupEmpty              ConsumerGroupState = "empty"
)

// ConsumerMember holds information about a consumer group member
type ConsumerMember struct {
	MemberID   string           `json:"member_id"`
	ClientID   string           `json:"client_id"`
	ClientHost string           `json:"client_host"`
	Assignment []TopicPartition `json:"assignment"`
}

// TopicPartition represents a topic and partition identifier
type TopicPartition struct {
	Topic     string `json:"topic"`
	Partition uint32 `json:"partition"`
}

// OffsetAndMetadata holds offset and metadata for a topic partition
type OffsetAndMetadata struct {
	Offset          uint64    `json:"offset"`
	Metadata        *string   `json:"metadata,omitempty"`
	CommitTimestamp time.Time `json:"commit_timestamp"`
}

// ClusterMetadata holds information about the cluster
type ClusterMetadata struct {
	ClusterID  string          `json:"cluster_id"`
	Brokers    []BrokerInfo    `json:"brokers"`
	Controller *BrokerInfo     `json:"controller,omitempty"`
	Topics     []TopicMetadata `json:"topics"`
}

// DeliveryReport represents a message delivery report
type DeliveryReport struct {
	MessageID string     `json:"message_id"`
	Topic     string     `json:"topic"`
	Partition uint32     `json:"partition"`
	Offset    uint64     `json:"offset"`
	Timestamp time.Time  `json:"timestamp"`
	Error     *string    `json:"error,omitempty"`
}

// TransactionInfo holds transaction information
type TransactionInfo struct {
	TransactionID string           `json:"transaction_id"`
	ProducerID    uint64           `json:"producer_id"`
	ProducerEpoch uint16           `json:"producer_epoch"`
	TimeoutMs     uint32           `json:"timeout_ms"`
	State         TransactionState `json:"state"`
}

// TransactionState represents transaction states
type TransactionState string

const (
	TransactionEmpty              TransactionState = "empty"
	TransactionOngoing            TransactionState = "ongoing"
	TransactionPrepareCommit      TransactionState = "prepare_commit"
	TransactionPrepareAbort       TransactionState = "prepare_abort"
	TransactionCompleteCommit     TransactionState = "complete_commit"
	TransactionCompleteAbort      TransactionState = "complete_abort"
	TransactionDead               TransactionState = "dead"
	TransactionPrepareEpochFence  TransactionState = "prepare_epoch_fence"
)

// PartitionAssignment holds partition assignment information
type PartitionAssignment struct {
	Topic      string   `json:"topic"`
	Partitions []uint32 `json:"partitions"`
}

// GroupAssignment holds consumer group assignment
type GroupAssignment struct {
	MemberID    string                `json:"member_id"`
	Assignments []PartitionAssignment `json:"assignments"`
}

// BrokerStats holds broker connection statistics
type BrokerStats struct {
	BrokerID         uint32    `json:"broker_id"`
	ConnectionCount  uint32    `json:"connection_count"`
	BytesSent        uint64    `json:"bytes_sent"`
	BytesReceived    uint64    `json:"bytes_received"`
	RequestsSent     uint64    `json:"requests_sent"`
	ResponsesReceived uint64   `json:"responses_received"`
	Errors           uint64    `json:"errors"`
	LastHeartbeat    time.Time `json:"last_heartbeat"`
}

// TopicConfig holds topic configuration
type TopicConfig struct {
	Name              string            `json:"name"`
	PartitionCount    uint32            `json:"partition_count"`
	ReplicationFactor uint16            `json:"replication_factor"`
	Config            map[string]string `json:"config"`
}

// AdminResult represents the result of an admin operation
type AdminResult[T any] struct {
	Success bool    `json:"success"`
	Result  *T      `json:"result,omitempty"`
	Error   *string `json:"error,omitempty"`
}

// Watermarks holds watermark information for a partition
type Watermarks struct {
	Low  uint64 `json:"low"`
	High uint64 `json:"high"`
}

// ConsumerLag holds consumer lag information
type ConsumerLag struct {
	Topic         string `json:"topic"`
	Partition     uint32 `json:"partition"`
	CurrentOffset uint64 `json:"current_offset"`
	LogEndOffset  uint64 `json:"log_end_offset"`
	Lag           uint64 `json:"lag"`
}

// HealthCheck represents a health check result
type HealthCheck struct {
	Status    HealthStatus      `json:"status"`
	Details   map[string]string `json:"details"`
	Timestamp time.Time         `json:"timestamp"`
}

// HealthStatus represents health status
type HealthStatus string

const (
	HealthStatusHealthy   HealthStatus = "healthy"
	HealthStatusDegraded  HealthStatus = "degraded"
	HealthStatusUnhealthy HealthStatus = "unhealthy"
	HealthStatusUnknown   HealthStatus = "unknown"
)

// FeatureFlags represents client capability flags
type FeatureFlags struct {
	IdempotentProducer    bool `json:"idempotent_producer"`
	TransactionalProducer bool `json:"transactional_producer"`
	ExactlyOnceSemantics  bool `json:"exactly_once_semantics"`
	StreamProcessing      bool `json:"stream_processing"`
	Compression           bool `json:"compression"`
	Encryption            bool `json:"encryption"`
	Monitoring            bool `json:"monitoring"`
}

// NewTopicPartition creates a new TopicPartition
func NewTopicPartition(topic string, partition uint32) TopicPartition {
	return TopicPartition{
		Topic:     topic,
		Partition: partition,
	}
}

// NewOffsetAndMetadata creates a new OffsetAndMetadata
func NewOffsetAndMetadata(offset uint64, metadata *string) OffsetAndMetadata {
	return OffsetAndMetadata{
		Offset:          offset,
		Metadata:        metadata,
		CommitTimestamp: time.Now(),
	}
}

// CalculateLag calculates consumer lag
func CalculateLag(currentOffset, logEndOffset uint64) uint64 {
	if logEndOffset >= currentOffset {
		return logEndOffset - currentOffset
	}
	return 0
}

// DefaultFeatureFlags returns default feature flags
func DefaultFeatureFlags() FeatureFlags {
	return FeatureFlags{
		IdempotentProducer:    true,
		TransactionalProducer: false,
		ExactlyOnceSemantics:  false,
		StreamProcessing:      true,
		Compression:           true,
		Encryption:            false,
		Monitoring:            true,
	}
}

// Protocol structures for consumer operations

// FetchRequest represents a request to fetch messages from a topic
type FetchRequest struct {
	Type          string    `json:"type"`
	Topic         string    `json:"topic"`
	ConsumerGroup string    `json:"consumer_group"`
	ConsumerID    string    `json:"consumer_id"`
	MaxMessages   int       `json:"max_messages"`
	MaxBytes      int       `json:"max_bytes"`
	TimeoutMs     int64     `json:"timeout_ms"`
	FromOffset    *uint64   `json:"from_offset,omitempty"`
	Timestamp     time.Time `json:"timestamp"`
}

// FetchResponse represents a response containing fetched messages
type FetchResponse struct {
	Success     bool               `json:"success"`
	Messages    []*Message         `json:"messages,omitempty"`
	Error       *string            `json:"error,omitempty"`
	EndOffset   uint64             `json:"end_offset"`
	PartitionID uint32             `json:"partition_id"`
	Watermarks  *Watermarks        `json:"watermarks,omitempty"`
}

// CommitRequest represents a request to commit offsets
type CommitRequest struct {
	Type          string                       `json:"type"`
	ConsumerGroup string                       `json:"consumer_group"`
	ConsumerID    string                       `json:"consumer_id"`
	Offsets       map[string]OffsetAndMetadata `json:"offsets"` // topic:partition -> offset
	Timestamp     time.Time                    `json:"timestamp"`
}

// CommitResponse represents a response to a commit request
type CommitResponse struct {
	Success bool                `json:"success"`
	Results map[string]*string  `json:"results,omitempty"` // topic:partition -> error (if any)
	Error   *string             `json:"error,omitempty"`
}

// SeekRequest represents a request to seek to a specific offset or timestamp
type SeekRequest struct {
	Type          string     `json:"type"`
	Topic         string     `json:"topic"`
	ConsumerGroup string     `json:"consumer_group"`
	ConsumerID    string     `json:"consumer_id"`
	Partition     uint32     `json:"partition"`
	Offset        *uint64    `json:"offset,omitempty"`
	Timestamp     *time.Time `json:"timestamp,omitempty"`
}

// SeekResponse represents a response to a seek request
type SeekResponse struct {
	Success   bool    `json:"success"`
	NewOffset uint64  `json:"new_offset"`
	Error     *string `json:"error,omitempty"`
}

// PartitionAssignmentRequest represents a request for partition assignment
type PartitionAssignmentRequest struct {
	Type          string    `json:"type"`
	ConsumerGroup string    `json:"consumer_group"`
	ConsumerID    string    `json:"consumer_id"`
	Topic         string    `json:"topic"`
	Timestamp     time.Time `json:"timestamp"`
}

// PartitionAssignmentResponse represents a response containing assigned partitions
type PartitionAssignmentResponse struct {
	Success    bool       `json:"success"`
	Partitions []uint32   `json:"partitions,omitempty"`
	Error      *string    `json:"error,omitempty"`
}

// ConsumerGroupJoinRequest represents a request to join a consumer group
type ConsumerGroupJoinRequest struct {
	Type          string            `json:"type"`
	ConsumerGroup string            `json:"consumer_group"`
	ConsumerID    string            `json:"consumer_id"`
	Topics        []string          `json:"topics"`
	Metadata      map[string]string `json:"metadata,omitempty"`
	Timestamp     time.Time         `json:"timestamp"`
}

// ConsumerGroupJoinResponse represents a response to joining a consumer group
type ConsumerGroupJoinResponse struct {
	Success       bool              `json:"success"`
	MemberID      string            `json:"member_id,omitempty"`
	GenerationID  uint32            `json:"generation_id,omitempty"`
	Assignment    []TopicPartition  `json:"assignment,omitempty"`
	Error         *string           `json:"error,omitempty"`
}

// ProduceRequest represents a request to produce messages to a topic
type ProduceRequest struct {
	Type        string     `json:"type"`
	Topic       string     `json:"topic"`
	ProducerID  string     `json:"producer_id"`
	Messages    []*Message `json:"messages"`
	AckLevel    AckLevel   `json:"ack_level"`
	Timestamp   time.Time  `json:"timestamp"`
	ClientID    string     `json:"client_id"`
	Idempotent  bool       `json:"idempotent"`
}

// ProduceResponse represents a response to a produce request
type ProduceResponse struct {
	Success     bool             `json:"success"`
	Results     []*MessageResult `json:"results,omitempty"`
	Error       *string          `json:"error,omitempty"`
	PartitionID uint32           `json:"partition_id"`
	BaseOffset  uint64           `json:"base_offset"`
}

// BatchProduceResult represents the result for individual messages in a batch
type BatchProduceResult struct {
	MessageID string    `json:"message_id"`
	Success   bool      `json:"success"`
	Offset    uint64    `json:"offset"`
	Partition uint32    `json:"partition"`
	Timestamp time.Time `json:"timestamp"`
	Error     *string   `json:"error,omitempty"`
}