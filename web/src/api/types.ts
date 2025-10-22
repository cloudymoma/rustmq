// API Response Types matching Rust backend structures

export interface ApiResponse<T> {
  success: boolean
  data: T | null
  error: string | null
  leader_hint: string | null
}

export interface HealthResponse {
  status: string
  version: string
  uptime_seconds: number
  is_leader: boolean
  raft_term: number
}

export interface BrokerStatus {
  id: string
  host: string
  port_quic: number
  port_rpc: number
  rack_id: string
  online: boolean
}

export interface TopicSummary {
  name: string
  partitions: number
  replication_factor: number
  created_at: string
}

export interface ClusterStatus {
  brokers: BrokerStatus[]
  topics: TopicSummary[]
  leader: string | null
  term: number
  healthy: boolean
}

export interface PartitionAssignment {
  partition: number
  leader: string
  replicas: string[]
  in_sync_replicas: string[]
  leader_epoch: number
}

export interface TopicConfig {
  retention_ms: number | null
  segment_bytes: number | null
  compression_type: string | null
}

export interface TopicDetail {
  name: string
  partitions: number
  replication_factor: number
  config: TopicConfig | null
  created_at: string
  partition_assignments: PartitionAssignment[]
}

export interface CreateTopicRequest {
  name: string
  partitions: number
  replication_factor: number
  retention_ms?: number
  segment_bytes?: number
  compression_type?: string
}
