package rustmq

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

// AdminClient provides access to RustMQ's Admin REST API for cluster management,
// security operations, and monitoring.
//
// Example usage:
//
//	client, err := rustmq.NewAdminClient("https://localhost:8080", nil)
//	if err != nil {
//		log.Fatal(err)
//	}
//	defer client.Close()
//
//	// Get cluster status
//	status, err := client.GetClusterStatus(context.Background())
//	if err != nil {
//		log.Fatal(err)
//	}
//	fmt.Printf("Cluster has %d brokers\n", len(status.Brokers))
type AdminClient struct {
	baseURL    string
	httpClient *http.Client
}

// AdminClientConfig contains configuration options for the AdminClient.
type AdminClientConfig struct {
	// TLS configuration for secure connections
	TLSConfig *tls.Config

	// Timeout for HTTP requests (default: 30 seconds)
	Timeout time.Duration

	// Custom HTTP transport for advanced configuration
	Transport http.RoundTripper
}

// NewAdminClient creates a new AdminClient with the given base URL and optional configuration.
//
// The baseURL should include the protocol (http:// or https://) and the host:port of the
// Admin API server (e.g., "https://localhost:8080").
//
// If config is nil, default values will be used (30 second timeout, system TLS config).
func NewAdminClient(baseURL string, config *AdminClientConfig) (*AdminClient, error) {
	if baseURL == "" {
		return nil, fmt.Errorf("baseURL cannot be empty")
	}

	// Validate URL format
	_, err := url.Parse(baseURL)
	if err != nil {
		return nil, fmt.Errorf("invalid baseURL: %w", err)
	}

	// Apply default config if not provided
	if config == nil {
		config = &AdminClientConfig{
			Timeout: 30 * time.Second,
		}
	}

	// Create HTTP client with configuration
	transport := config.Transport
	if transport == nil {
		transport = &http.Transport{
			TLSClientConfig: config.TLSConfig,
		}
	}

	timeout := config.Timeout
	if timeout == 0 {
		timeout = 30 * time.Second
	}

	httpClient := &http.Client{
		Transport: transport,
		Timeout:   timeout,
	}

	return &AdminClient{
		baseURL:    baseURL,
		httpClient: httpClient,
	}, nil
}

// Close releases resources associated with the AdminClient.
func (c *AdminClient) Close() error {
	c.httpClient.CloseIdleConnections()
	return nil
}

// ==================== Common Types ====================

// ApiResponse is the standard response wrapper for all Admin API endpoints.
type ApiResponse struct {
	Success    bool        `json:"success"`
	Data       interface{} `json:"data,omitempty"`
	Error      *string     `json:"error,omitempty"`
	LeaderHint *string     `json:"leader_hint,omitempty"`
}

// ==================== Health & Cluster Endpoints ====================

// HealthResponse contains health check information for the controller.
type HealthResponse struct {
	Status        string `json:"status"`
	Version       string `json:"version"`
	UptimeSeconds uint64 `json:"uptime_seconds"`
	IsLeader      bool   `json:"is_leader"`
	RaftTerm      uint64 `json:"raft_term"`
}

// BrokerStatus contains information about a broker in the cluster.
type BrokerStatus struct {
	ID       string `json:"id"`
	Host     string `json:"host"`
	PortQuic uint16 `json:"port_quic"`
	PortRpc  uint16 `json:"port_rpc"`
	RackID   string `json:"rack_id"`
	Online   bool   `json:"online"`
}

// ClusterStatus contains comprehensive cluster status information.
type ClusterStatus struct {
	Brokers []BrokerStatus `json:"brokers"`
	Topics  []TopicSummary `json:"topics"`
	Leader  *string        `json:"leader,omitempty"`
	Term    uint64         `json:"term"`
	Healthy bool           `json:"healthy"`
}

// TopicSummary contains summary information about a topic.
type TopicSummary struct {
	Name              string    `json:"name"`
	Partitions        uint32    `json:"partitions"`
	ReplicationFactor uint32    `json:"replication_factor"`
	CreatedAt         time.Time `json:"created_at"`
}

// CreateTopicRequest contains parameters for creating a new topic.
type CreateTopicRequest struct {
	Name              string  `json:"name"`
	Partitions        uint32  `json:"partitions"`
	ReplicationFactor uint32  `json:"replication_factor"`
	RetentionMs       *uint64 `json:"retention_ms,omitempty"`
	SegmentBytes      *uint64 `json:"segment_bytes,omitempty"`
	CompressionType   *string `json:"compression_type,omitempty"`
}

// TopicDetail contains detailed information about a topic including partition assignments.
type TopicDetail struct {
	Name                 string                       `json:"name"`
	Partitions           uint32                       `json:"partitions"`
	ReplicationFactor    uint32                       `json:"replication_factor"`
	Config               map[string]interface{}       `json:"config,omitempty"`
	CreatedAt            time.Time                    `json:"created_at"`
	PartitionAssignments []TopicPartitionAssignment   `json:"partition_assignments"`
}

// TopicPartitionAssignment contains assignment information for a topic partition.
// This is different from PartitionAssignment in types.go which is for consumer groups.
type TopicPartitionAssignment struct {
	Partition       uint32   `json:"partition"`
	Leader          string   `json:"leader"`
	Replicas        []string `json:"replicas"`
	InSyncReplicas  []string `json:"in_sync_replicas"`
	LeaderEpoch     uint64   `json:"leader_epoch"`
}

// Health performs a health check on the Admin API server.
func (c *AdminClient) Health(ctx context.Context) (*HealthResponse, error) {
	var result HealthResponse
	err := c.doRequest(ctx, "GET", "/health", nil, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// GetClusterStatus retrieves comprehensive cluster status including brokers and topics.
func (c *AdminClient) GetClusterStatus(ctx context.Context) (*ClusterStatus, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/cluster", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	// Convert data to ClusterStatus
	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var status ClusterStatus
	if err := json.Unmarshal(data, &status); err != nil {
		return nil, fmt.Errorf("failed to unmarshal cluster status: %w", err)
	}

	return &status, nil
}

// ListBrokers retrieves a list of all brokers in the cluster with their health status.
func (c *AdminClient) ListBrokers(ctx context.Context) ([]BrokerStatus, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/brokers", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	// Convert data to broker list
	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var brokers []BrokerStatus
	if err := json.Unmarshal(data, &brokers); err != nil {
		return nil, fmt.Errorf("failed to unmarshal brokers: %w", err)
	}

	return brokers, nil
}

// ListTopics retrieves a list of all topics in the cluster.
func (c *AdminClient) ListTopics(ctx context.Context) ([]TopicSummary, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/topics", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	// Convert data to topic list
	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var topics []TopicSummary
	if err := json.Unmarshal(data, &topics); err != nil {
		return nil, fmt.Errorf("failed to unmarshal topics: %w", err)
	}

	return topics, nil
}

// CreateTopic creates a new topic with the specified configuration.
func (c *AdminClient) CreateTopic(ctx context.Context, req *CreateTopicRequest) error {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/topics", req, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// DeleteTopic deletes a topic by name.
func (c *AdminClient) DeleteTopic(ctx context.Context, topicName string) error {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/topics/%s", url.PathEscape(topicName))
	err := c.doRequest(ctx, "DELETE", path, nil, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// GetTopicDetail retrieves detailed information about a specific topic.
func (c *AdminClient) GetTopicDetail(ctx context.Context, topicName string) (*TopicDetail, error) {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/topics/%s", url.PathEscape(topicName))
	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	// Convert data to TopicDetail
	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var detail TopicDetail
	if err := json.Unmarshal(data, &detail); err != nil {
		return nil, fmt.Errorf("failed to unmarshal topic detail: %w", err)
	}

	return &detail, nil
}

// ==================== Security - CA Endpoints ====================

// CaInitRequest contains parameters for initializing a new Certificate Authority.
type CaInitRequest struct {
	CommonName   string  `json:"common_name"`
	Organization *string `json:"organization,omitempty"`
	Country      *string `json:"country,omitempty"`
	ValidityDays *uint32 `json:"validity_days,omitempty"`
	KeySize      *uint32 `json:"key_size,omitempty"`
}

// CaInfo contains information about a Certificate Authority.
type CaInfo struct {
	CaID         string    `json:"ca_id"`
	CommonName   string    `json:"common_name"`
	Organization *string   `json:"organization,omitempty"`
	Subject      string    `json:"subject"`
	Issuer       string    `json:"issuer"`
	SerialNumber string    `json:"serial_number"`
	NotBefore    time.Time `json:"not_before"`
	NotAfter     time.Time `json:"not_after"`
	KeyUsage     []string  `json:"key_usage"`
	IsCA         bool      `json:"is_ca"`
	PathLength   *uint32   `json:"path_length,omitempty"`
	Status       string    `json:"status"`
	Fingerprint  string    `json:"fingerprint"`
}

// InitCA initializes a new Certificate Authority.
func (c *AdminClient) InitCA(ctx context.Context, req *CaInitRequest) (*CaInfo, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/ca/init", req, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result CaInfo
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// GetCAInfo retrieves information about the Certificate Authority.
func (c *AdminClient) GetCAInfo(ctx context.Context) (*CaInfo, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/security/ca/info", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result CaInfo
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// RefreshCA refreshes the CA certificate chain.
func (c *AdminClient) RefreshCA(ctx context.Context) error {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/ca/refresh", nil, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// ==================== Security - Certificate Endpoints ====================

// IssueCertificateRequest contains parameters for issuing a new certificate.
type IssueCertificateRequest struct {
	CaID              string   `json:"ca_id"`
	CommonName        string   `json:"common_name"`
	SubjectAltNames   []string `json:"subject_alt_names,omitempty"`
	Organization      *string  `json:"organization,omitempty"`
	Role              *string  `json:"role,omitempty"`
	ValidityDays      *uint32  `json:"validity_days,omitempty"`
	KeyUsage          []string `json:"key_usage,omitempty"`
	ExtendedKeyUsage  []string `json:"extended_key_usage,omitempty"`
}

// AdminCertificateInfo contains information about a certificate from the Admin API.
// This is different from CertificateInfo in security.go which is for TLS client certificates.
type AdminCertificateInfo struct {
	CertificateID     string     `json:"certificate_id"`
	CommonName        string     `json:"common_name"`
	Subject           string     `json:"subject"`
	Issuer            string     `json:"issuer"`
	SerialNumber      string     `json:"serial_number"`
	NotBefore         time.Time  `json:"not_before"`
	NotAfter          time.Time  `json:"not_after"`
	Status            string     `json:"status"`
	Role              *string    `json:"role,omitempty"`
	Fingerprint       string     `json:"fingerprint"`
	KeyUsage          []string   `json:"key_usage,omitempty"`
	ExtendedKeyUsage  []string   `json:"extended_key_usage,omitempty"`
	SubjectAltNames   []string   `json:"subject_alt_names,omitempty"`
	CertificateChain  []string   `json:"certificate_chain,omitempty"`
	RevocationReason  *string    `json:"revocation_reason,omitempty"`
	RevocationTime    *time.Time `json:"revocation_time,omitempty"`
}

// IssueCertificate issues a new certificate from the specified CA.
func (c *AdminClient) IssueCertificate(ctx context.Context, req *IssueCertificateRequest) (*AdminCertificateInfo, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/certificates/issue", req, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AdminCertificateInfo
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// ListCertificates retrieves a list of all certificates.
func (c *AdminClient) ListCertificates(ctx context.Context) ([]AdminCertificateInfo, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/security/certificates", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var certs []AdminCertificateInfo
	if err := json.Unmarshal(data, &certs); err != nil {
		return nil, fmt.Errorf("failed to unmarshal certificates: %w", err)
	}

	return certs, nil
}

// RevokeCertificate revokes a certificate by serial number.
func (c *AdminClient) RevokeCertificate(ctx context.Context, serial string, reason string) error {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/certificates/%s", url.PathEscape(serial))

	body := map[string]interface{}{
		"reason": reason,
	}

	err := c.doRequest(ctx, "DELETE", path, body, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// GetCertificateStatus retrieves the status of a certificate by serial number.
func (c *AdminClient) GetCertificateStatus(ctx context.Context, serial string) (*AdminCertificateInfo, error) {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/certificates/%s/status", url.PathEscape(serial))
	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AdminCertificateInfo
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// GetCertificateChain retrieves the certificate chain for a certificate.
func (c *AdminClient) GetCertificateChain(ctx context.Context, serial string) ([]string, error) {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/certificates/%s/chain", url.PathEscape(serial))
	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var chain []string
	if err := json.Unmarshal(data, &chain); err != nil {
		return nil, fmt.Errorf("failed to unmarshal certificate chain: %w", err)
	}

	return chain, nil
}

// ==================== Security - ACL Endpoints ====================

// CreateAclRuleRequest contains parameters for creating a new ACL rule.
type CreateAclRuleRequest struct {
	Principal       string            `json:"principal"`
	ResourcePattern string            `json:"resource_pattern"`
	ResourceType    string            `json:"resource_type"`
	Operation       string            `json:"operation"`
	Effect          string            `json:"effect"`
	Conditions      map[string]string `json:"conditions,omitempty"`
}

// UpdateAclRuleRequest contains parameters for updating an ACL rule.
type UpdateAclRuleRequest struct {
	Principal       *string           `json:"principal,omitempty"`
	ResourcePattern *string           `json:"resource_pattern,omitempty"`
	ResourceType    *string           `json:"resource_type,omitempty"`
	Operation       *string           `json:"operation,omitempty"`
	Effect          *string           `json:"effect,omitempty"`
	Conditions      map[string]string `json:"conditions,omitempty"`
}

// AclRule contains information about an ACL rule.
type AclRule struct {
	RuleID          string            `json:"rule_id"`
	Principal       string            `json:"principal"`
	ResourcePattern string            `json:"resource_pattern"`
	ResourceType    string            `json:"resource_type"`
	Operation       string            `json:"operation"`
	Effect          string            `json:"effect"`
	Conditions      map[string]string `json:"conditions"`
	CreatedAt       time.Time         `json:"created_at"`
	UpdatedAt       time.Time         `json:"updated_at"`
	Version         uint64            `json:"version"`
}

// EvaluatePermissionRequest contains parameters for evaluating a permission.
type EvaluatePermissionRequest struct {
	Principal string            `json:"principal"`
	Resource  string            `json:"resource"`
	Operation string            `json:"operation"`
	Context   map[string]string `json:"context,omitempty"`
}

// EvaluatePermissionResponse contains the result of permission evaluation.
type EvaluatePermissionResponse struct {
	Allowed           bool     `json:"allowed"`
	Effect            string   `json:"effect"`
	MatchedRules      []string `json:"matched_rules"`
	EvaluationTimeNs  uint64   `json:"evaluation_time_ns"`
}

// CreateACL creates a new ACL rule.
func (c *AdminClient) CreateACL(ctx context.Context, req *CreateAclRuleRequest) (*AclRule, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/acl", req, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AclRule
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// UpdateACL updates an existing ACL rule.
func (c *AdminClient) UpdateACL(ctx context.Context, ruleID string, req *UpdateAclRuleRequest) (*AclRule, error) {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/acl/%s", url.PathEscape(ruleID))
	err := c.doRequest(ctx, "PUT", path, req, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AclRule
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// DeleteACL deletes an ACL rule.
func (c *AdminClient) DeleteACL(ctx context.Context, ruleID string) error {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/acl/%s", url.PathEscape(ruleID))
	err := c.doRequest(ctx, "DELETE", path, nil, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// ListACLs retrieves a list of ACL rules with optional filters.
func (c *AdminClient) ListACLs(ctx context.Context, filters map[string]string) ([]AclRule, error) {
	var resp ApiResponse

	// Build query parameters from filters
	queryParams := url.Values{}
	for key, value := range filters {
		queryParams.Add(key, value)
	}

	path := "/api/v1/security/acl"
	if len(queryParams) > 0 {
		path = fmt.Sprintf("%s?%s", path, queryParams.Encode())
	}

	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	data, err := json.Marshal(resp.Data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	var rules []AclRule
	if err := json.Unmarshal(data, &rules); err != nil {
		return nil, fmt.Errorf("failed to unmarshal ACL rules: %w", err)
	}

	return rules, nil
}

// EvaluatePermission evaluates whether a principal has permission for an operation on a resource.
func (c *AdminClient) EvaluatePermission(ctx context.Context, req *EvaluatePermissionRequest) (*EvaluatePermissionResponse, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/acl/evaluate", req, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result EvaluatePermissionResponse
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// SyncACLs forces synchronization of ACL rules to all brokers.
func (c *AdminClient) SyncACLs(ctx context.Context) error {
	var resp ApiResponse
	err := c.doRequest(ctx, "POST", "/api/v1/security/acl/sync", nil, &resp)
	if err != nil {
		return err
	}

	if !resp.Success {
		return c.extractError(&resp)
	}

	return nil
}

// ==================== Security - Audit Endpoints ====================

// AuditLogQuery contains parameters for querying audit logs.
type AuditLogQuery struct {
	StartTime *time.Time `json:"start_time,omitempty"`
	EndTime   *time.Time `json:"end_time,omitempty"`
	EventType *string    `json:"event_type,omitempty"`
	Principal *string    `json:"principal,omitempty"`
	Limit     *uint32    `json:"limit,omitempty"`
	Offset    *uint32    `json:"offset,omitempty"`
}

// AuditLogEntry represents a single audit log entry.
type AuditLogEntry struct {
	Timestamp  time.Time         `json:"timestamp"`
	EventType  string            `json:"event_type"`
	Principal  *string           `json:"principal,omitempty"`
	Resource   *string           `json:"resource,omitempty"`
	Operation  *string           `json:"operation,omitempty"`
	Result     string            `json:"result"`
	Details    map[string]string `json:"details"`
	ClientIP   *string           `json:"client_ip,omitempty"`
	UserAgent  *string           `json:"user_agent,omitempty"`
}

// AuditLogResponse contains audit log query results.
type AuditLogResponse struct {
	Events     []AuditLogEntry `json:"events"`
	TotalCount uint64          `json:"total_count"`
	HasMore    bool            `json:"has_more"`
}

// GetAuditLogs retrieves audit logs with optional filters.
func (c *AdminClient) GetAuditLogs(ctx context.Context, query *AuditLogQuery) (*AuditLogResponse, error) {
	var resp ApiResponse

	// Build query parameters
	queryParams := url.Values{}
	if query != nil {
		if query.StartTime != nil {
			queryParams.Add("start_time", query.StartTime.Format(time.RFC3339))
		}
		if query.EndTime != nil {
			queryParams.Add("end_time", query.EndTime.Format(time.RFC3339))
		}
		if query.EventType != nil {
			queryParams.Add("event_type", *query.EventType)
		}
		if query.Principal != nil {
			queryParams.Add("principal", *query.Principal)
		}
		if query.Limit != nil {
			queryParams.Add("limit", fmt.Sprintf("%d", *query.Limit))
		}
		if query.Offset != nil {
			queryParams.Add("offset", fmt.Sprintf("%d", *query.Offset))
		}
	}

	path := "/api/v1/security/audit/logs"
	if len(queryParams) > 0 {
		path = fmt.Sprintf("%s?%s", path, queryParams.Encode())
	}

	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AuditLogResponse
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// GetAuditEvents retrieves audit events (alias for GetAuditLogs).
func (c *AdminClient) GetAuditEvents(ctx context.Context, query *AuditLogQuery) (*AuditLogResponse, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/security/audit/events", query, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AuditLogResponse
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// GetAuditTrail retrieves the audit trail for a specific principal.
func (c *AdminClient) GetAuditTrail(ctx context.Context, principal string) (*AuditLogResponse, error) {
	var resp ApiResponse
	path := fmt.Sprintf("/api/v1/security/audit/trail/%s", url.PathEscape(principal))
	err := c.doRequest(ctx, "GET", path, nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AuditLogResponse
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// AuditMetrics contains audit system metrics.
type AuditMetrics struct {
	TotalEvents      uint64            `json:"total_events"`
	EventsByType     map[string]uint64 `json:"events_by_type"`
	EventsByResult   map[string]uint64 `json:"events_by_result"`
	EventsPerHour    []uint64          `json:"events_per_hour"`
	TopPrincipals    []string          `json:"top_principals"`
	TopResources     []string          `json:"top_resources"`
}

// GetAuditMetrics retrieves audit system metrics.
func (c *AdminClient) GetAuditMetrics(ctx context.Context) (*AuditMetrics, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/security/audit/metrics", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AuditMetrics
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// AuditConfig contains audit configuration.
type AuditConfig struct {
	Enabled          bool     `json:"enabled"`
	LogLevel         string   `json:"log_level"`
	RetentionDays    uint32   `json:"retention_days"`
	EventTypes       []string `json:"event_types"`
	ExcludedPaths    []string `json:"excluded_paths"`
}

// GetAuditConfig retrieves the current audit configuration.
func (c *AdminClient) GetAuditConfig(ctx context.Context) (*AuditConfig, error) {
	var resp ApiResponse
	err := c.doRequest(ctx, "GET", "/api/v1/security/audit/config", nil, &resp)
	if err != nil {
		return nil, err
	}

	if !resp.Success {
		return nil, c.extractError(&resp)
	}

	var result AuditConfig
	_, err = c.unmarshalData(resp.Data, &result)
	if err != nil {
		return nil, err
	}
	return &result, nil
}

// ==================== Private Helper Methods ====================

// doRequest performs an HTTP request to the Admin API.
func (c *AdminClient) doRequest(ctx context.Context, method, path string, body interface{}, result interface{}) error {
	// Build full URL
	fullURL := c.baseURL + path

	// Prepare request body
	var reqBody io.Reader
	if body != nil {
		jsonData, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("failed to marshal request body: %w", err)
		}
		reqBody = bytes.NewBuffer(jsonData)
	}

	// Create HTTP request
	req, err := http.NewRequestWithContext(ctx, method, fullURL, reqBody)
	if err != nil {
		return fmt.Errorf("failed to create HTTP request: %w", err)
	}

	// Set headers
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")

	// Execute request
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("HTTP request failed: %w", err)
	}
	defer resp.Body.Close()

	// Read response body
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read response body: %w", err)
	}

	// Check HTTP status code
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("HTTP request failed with status %d: %s", resp.StatusCode, string(respBody))
	}

	// Decode response
	if result != nil {
		if err := json.Unmarshal(respBody, result); err != nil {
			return fmt.Errorf("failed to decode response: %w (body: %s)", err, string(respBody))
		}
	}

	return nil
}

// extractError extracts an error message from an ApiResponse.
func (c *AdminClient) extractError(resp *ApiResponse) error {
	if resp.Error != nil {
		return fmt.Errorf("API error: %s", *resp.Error)
	}
	return fmt.Errorf("API request failed with unknown error")
}

// unmarshalData is a generic helper to unmarshal response data into a typed struct.
func (c *AdminClient) unmarshalData(data interface{}, target interface{}) (interface{}, error) {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal response data: %w", err)
	}

	if err := json.Unmarshal(jsonData, target); err != nil {
		return nil, fmt.Errorf("failed to unmarshal data: %w", err)
	}

	return target, nil
}
