package rustmq

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// TestNewAdminClient tests the creation of a new AdminClient.
func TestNewAdminClient(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		baseURL   string
		config    *AdminClientConfig
		expectErr bool
	}{
		{
			name:      "valid URL with nil config",
			baseURL:   "https://localhost:8080",
			config:    nil,
			expectErr: false,
		},
		{
			name:    "valid URL with custom config",
			baseURL: "https://localhost:8080",
			config: &AdminClientConfig{
				Timeout: 10 * time.Second,
			},
			expectErr: false,
		},
		{
			name:      "empty URL",
			baseURL:   "",
			config:    nil,
			expectErr: true,
		},
		{
			name:      "invalid URL",
			baseURL:   "://invalid",
			config:    nil,
			expectErr: true,
		},
	}

	for _, tt := range tests {
		tt := tt // capture range variable
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			client, err := NewAdminClient(tt.baseURL, tt.config)
			if tt.expectErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
				if client == nil {
					t.Errorf("expected client, got nil")
				}
				if client != nil {
					client.Close()
				}
			}
		})
	}
}

// TestHealthEndpoint tests the Health endpoint.
func TestHealthEndpoint(t *testing.T) {
	t.Parallel()

	// Create mock server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		if r.Method != "GET" {
			t.Errorf("unexpected method: %s", r.Method)
		}

		response := HealthResponse{
			Status:        "ok",
			Version:       "1.0.0",
			UptimeSeconds: 3600,
			IsLeader:      true,
			RaftTerm:      5,
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, err := NewAdminClient(server.URL, nil)
	if err != nil {
		t.Fatalf("failed to create client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	health, err := client.Health(ctx)
	if err != nil {
		t.Fatalf("Health() failed: %v", err)
	}

	if health.Status != "ok" {
		t.Errorf("expected status 'ok', got %s", health.Status)
	}
	if health.Version != "1.0.0" {
		t.Errorf("expected version '1.0.0', got %s", health.Version)
	}
	if !health.IsLeader {
		t.Errorf("expected IsLeader to be true")
	}
	if health.RaftTerm != 5 {
		t.Errorf("expected RaftTerm 5, got %d", health.RaftTerm)
	}
}

// TestGetClusterStatus tests the GetClusterStatus endpoint.
func TestGetClusterStatus(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/cluster" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}

		leader := "controller-1"
		response := ApiResponse{
			Success: true,
			Data: ClusterStatus{
				Brokers: []BrokerStatus{
					{
						ID:       "broker-1",
						Host:     "localhost",
						PortQuic: 9092,
						PortRpc:  9093,
						RackID:   "rack-1",
						Online:   true,
					},
				},
				Topics:  []TopicSummary{},
				Leader:  &leader,
				Term:    5,
				Healthy: true,
			},
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, err := NewAdminClient(server.URL, nil)
	if err != nil {
		t.Fatalf("failed to create client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()
	status, err := client.GetClusterStatus(ctx)
	if err != nil {
		t.Fatalf("GetClusterStatus() failed: %v", err)
	}

	if len(status.Brokers) != 1 {
		t.Errorf("expected 1 broker, got %d", len(status.Brokers))
	}
	if !status.Healthy {
		t.Errorf("expected cluster to be healthy")
	}
	if status.Term != 5 {
		t.Errorf("expected term 5, got %d", status.Term)
	}
}

// TestListBrokers tests the ListBrokers endpoint.
func TestListBrokers(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		response := ApiResponse{
			Success: true,
			Data: []BrokerStatus{
				{ID: "broker-1", Host: "localhost", PortQuic: 9092, PortRpc: 9093, RackID: "rack-1", Online: true},
				{ID: "broker-2", Host: "localhost", PortQuic: 9192, PortRpc: 9193, RackID: "rack-2", Online: false},
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	brokers, err := client.ListBrokers(context.Background())
	if err != nil {
		t.Fatalf("ListBrokers() failed: %v", err)
	}

	if len(brokers) != 2 {
		t.Errorf("expected 2 brokers, got %d", len(brokers))
	}
	if !brokers[0].Online {
		t.Errorf("expected broker-1 to be online")
	}
	if brokers[1].Online {
		t.Errorf("expected broker-2 to be offline")
	}
}

// TestCreateTopic tests the CreateTopic endpoint.
func TestCreateTopic(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" {
			t.Errorf("expected POST, got %s", r.Method)
		}

		var req CreateTopicRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("failed to decode request: %v", err)
		}

		if req.Name != "test-topic" {
			t.Errorf("expected topic name 'test-topic', got %s", req.Name)
		}

		response := ApiResponse{
			Success: true,
			Data:    "Topic 'test-topic' created",
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	retention := uint64(86400000)
	req := &CreateTopicRequest{
		Name:              "test-topic",
		Partitions:        3,
		ReplicationFactor: 2,
		RetentionMs:       &retention,
	}

	err := client.CreateTopic(context.Background(), req)
	if err != nil {
		t.Fatalf("CreateTopic() failed: %v", err)
	}
}

// TestDeleteTopic tests the DeleteTopic endpoint.
func TestDeleteTopic(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "DELETE" {
			t.Errorf("expected DELETE, got %s", r.Method)
		}
		if !strings.Contains(r.URL.Path, "test-topic") {
			t.Errorf("expected path to contain 'test-topic', got %s", r.URL.Path)
		}

		response := ApiResponse{
			Success: true,
			Data:    "Topic deleted",
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	err := client.DeleteTopic(context.Background(), "test-topic")
	if err != nil {
		t.Fatalf("DeleteTopic() failed: %v", err)
	}
}

// TestGetTopicDetail tests the GetTopicDetail endpoint.
func TestGetTopicDetail(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		response := ApiResponse{
			Success: true,
			Data: TopicDetail{
				Name:              "test-topic",
				Partitions:        3,
				ReplicationFactor: 2,
				CreatedAt:         time.Now(),
				PartitionAssignments: []TopicPartitionAssignment{
					{
						Partition:      0,
						Leader:         "broker-1",
						Replicas:       []string{"broker-1", "broker-2"},
						InSyncReplicas: []string{"broker-1", "broker-2"},
						LeaderEpoch:    1,
					},
				},
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	detail, err := client.GetTopicDetail(context.Background(), "test-topic")
	if err != nil {
		t.Fatalf("GetTopicDetail() failed: %v", err)
	}

	if detail.Name != "test-topic" {
		t.Errorf("expected name 'test-topic', got %s", detail.Name)
	}
	if detail.Partitions != 3 {
		t.Errorf("expected 3 partitions, got %d", detail.Partitions)
	}
	if len(detail.PartitionAssignments) != 1 {
		t.Errorf("expected 1 partition assignment, got %d", len(detail.PartitionAssignments))
	}
}

// TestInitCA tests the InitCA endpoint.
func TestInitCA(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req CaInitRequest
		json.NewDecoder(r.Body).Decode(&req)

		pathLen := uint32(0)
		response := ApiResponse{
			Success: true,
			Data: CaInfo{
				CaID:         "ca-1",
				CommonName:   req.CommonName,
				Subject:      fmt.Sprintf("CN=%s", req.CommonName),
				Issuer:       fmt.Sprintf("CN=%s", req.CommonName),
				SerialNumber: "1234567890",
				NotBefore:    time.Now(),
				NotAfter:     time.Now().Add(365 * 24 * time.Hour),
				KeyUsage:     []string{"keyCertSign", "cRLSign"},
				IsCA:         true,
				PathLength:   &pathLen,
				Status:       "Active",
				Fingerprint:  "abc123",
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	org := "RustMQ"
	req := &CaInitRequest{
		CommonName:   "RustMQ Root CA",
		Organization: &org,
	}

	caInfo, err := client.InitCA(context.Background(), req)
	if err != nil {
		t.Fatalf("InitCA() failed: %v", err)
	}

	if caInfo.CommonName != "RustMQ Root CA" {
		t.Errorf("expected common name 'RustMQ Root CA', got %s", caInfo.CommonName)
	}
	if !caInfo.IsCA {
		t.Errorf("expected IsCA to be true")
	}
}

// TestIssueCertificate tests the IssueCertificate endpoint.
func TestIssueCertificate(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req IssueCertificateRequest
		json.NewDecoder(r.Body).Decode(&req)

		role := "broker"
		response := ApiResponse{
			Success: true,
			Data: AdminCertificateInfo{
				CertificateID: "cert-1",
				CommonName:    req.CommonName,
				Subject:       fmt.Sprintf("CN=%s", req.CommonName),
				Issuer:        "CN=RustMQ Root CA",
				SerialNumber:  "0987654321",
				NotBefore:     time.Now(),
				NotAfter:      time.Now().Add(365 * 24 * time.Hour),
				Status:        "Active",
				Role:          &role,
				Fingerprint:   "def456",
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	role := "broker"
	req := &IssueCertificateRequest{
		CaID:       "ca-1",
		CommonName: "broker-1.rustmq.local",
		Role:       &role,
	}

	certInfo, err := client.IssueCertificate(context.Background(), req)
	if err != nil {
		t.Fatalf("IssueCertificate() failed: %v", err)
	}

	if certInfo.CommonName != "broker-1.rustmq.local" {
		t.Errorf("expected common name 'broker-1.rustmq.local', got %s", certInfo.CommonName)
	}
	if certInfo.Status != "Active" {
		t.Errorf("expected status 'Active', got %s", certInfo.Status)
	}
}

// TestCreateACL tests the CreateACL endpoint.
func TestCreateACL(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req CreateAclRuleRequest
		json.NewDecoder(r.Body).Decode(&req)

		response := ApiResponse{
			Success: true,
			Data: AclRule{
				RuleID:          "rule-1",
				Principal:       req.Principal,
				ResourcePattern: req.ResourcePattern,
				ResourceType:    req.ResourceType,
				Operation:       req.Operation,
				Effect:          req.Effect,
				Conditions:      req.Conditions,
				CreatedAt:       time.Now(),
				UpdatedAt:       time.Now(),
				Version:         1,
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	req := &CreateAclRuleRequest{
		Principal:       "user@example.com",
		ResourcePattern: "topic.events.*",
		ResourceType:    "topic",
		Operation:       "read",
		Effect:          "allow",
	}

	rule, err := client.CreateACL(context.Background(), req)
	if err != nil {
		t.Fatalf("CreateACL() failed: %v", err)
	}

	if rule.Principal != "user@example.com" {
		t.Errorf("expected principal 'user@example.com', got %s", rule.Principal)
	}
	if rule.Effect != "allow" {
		t.Errorf("expected effect 'allow', got %s", rule.Effect)
	}
}

// TestEvaluatePermission tests the EvaluatePermission endpoint.
func TestEvaluatePermission(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		response := ApiResponse{
			Success: true,
			Data: EvaluatePermissionResponse{
				Allowed:          true,
				Effect:           "allow",
				MatchedRules:     []string{"rule-1"},
				EvaluationTimeNs: 1000,
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	req := &EvaluatePermissionRequest{
		Principal: "user@example.com",
		Resource:  "topic.events.user-signup",
		Operation: "read",
	}

	result, err := client.EvaluatePermission(context.Background(), req)
	if err != nil {
		t.Fatalf("EvaluatePermission() failed: %v", err)
	}

	if !result.Allowed {
		t.Errorf("expected permission to be allowed")
	}
	if result.Effect != "allow" {
		t.Errorf("expected effect 'allow', got %s", result.Effect)
	}
	if len(result.MatchedRules) != 1 {
		t.Errorf("expected 1 matched rule, got %d", len(result.MatchedRules))
	}
}

// TestGetAuditLogs tests the GetAuditLogs endpoint.
func TestGetAuditLogs(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		eventType := "authentication"
		principal := "user@example.com"
		resource := "topic.events"
		response := ApiResponse{
			Success: true,
			Data: AuditLogResponse{
				Events: []AuditLogEntry{
					{
						Timestamp: time.Now(),
						EventType: eventType,
						Principal: &principal,
						Resource:  &resource,
						Result:    "success",
						Details:   map[string]string{"action": "login"},
					},
				},
				TotalCount: 1,
				HasMore:    false,
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	limit := uint32(10)
	query := &AuditLogQuery{
		Limit: &limit,
	}

	logs, err := client.GetAuditLogs(context.Background(), query)
	if err != nil {
		t.Fatalf("GetAuditLogs() failed: %v", err)
	}

	if len(logs.Events) != 1 {
		t.Errorf("expected 1 event, got %d", len(logs.Events))
	}
	if logs.TotalCount != 1 {
		t.Errorf("expected total count 1, got %d", logs.TotalCount)
	}
	if logs.HasMore {
		t.Errorf("expected HasMore to be false")
	}
}

// TestErrorHandling tests error handling in the AdminClient.
func TestErrorHandling(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		errMsg := "topic already exists"
		response := ApiResponse{
			Success: false,
			Error:   &errMsg,
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	req := &CreateTopicRequest{
		Name:              "existing-topic",
		Partitions:        1,
		ReplicationFactor: 1,
	}

	err := client.CreateTopic(context.Background(), req)
	if err == nil {
		t.Fatalf("expected error, got nil")
	}
	if !strings.Contains(err.Error(), "topic already exists") {
		t.Errorf("expected error message to contain 'topic already exists', got: %v", err)
	}
}

// TestContextCancellation tests that context cancellation works properly.
func TestContextCancellation(t *testing.T) {
	t.Parallel()

	// Create a server that delays response
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(100 * time.Millisecond)
		json.NewEncoder(w).Encode(ApiResponse{Success: true})
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	// Create a context that will be cancelled immediately
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := client.Health(ctx)
	if err == nil {
		t.Errorf("expected context cancellation error, got nil")
	}
}

// TestListACLsWithFilters tests the ListACLs endpoint with filters.
func TestListACLsWithFilters(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify query parameters
		query := r.URL.Query()
		if query.Get("principal") != "user@example.com" {
			t.Errorf("expected principal filter, got %s", query.Get("principal"))
		}

		response := ApiResponse{
			Success: true,
			Data: []AclRule{
				{
					RuleID:          "rule-1",
					Principal:       "user@example.com",
					ResourcePattern: "topic.events.*",
					ResourceType:    "topic",
					Operation:       "read",
					Effect:          "allow",
					Conditions:      map[string]string{},
					CreatedAt:       time.Now(),
					UpdatedAt:       time.Now(),
					Version:         1,
				},
			},
		}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	filters := map[string]string{
		"principal": "user@example.com",
	}

	rules, err := client.ListACLs(context.Background(), filters)
	if err != nil {
		t.Fatalf("ListACLs() failed: %v", err)
	}

	if len(rules) != 1 {
		t.Errorf("expected 1 rule, got %d", len(rules))
	}
	if rules[0].Principal != "user@example.com" {
		t.Errorf("expected principal 'user@example.com', got %s", rules[0].Principal)
	}
}

// TestHTTPErrorResponse tests handling of non-2xx HTTP responses.
func TestHTTPErrorResponse(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		w.Write([]byte("Internal Server Error"))
	}))
	defer server.Close()

	client, _ := NewAdminClient(server.URL, nil)
	defer client.Close()

	_, err := client.Health(context.Background())
	if err == nil {
		t.Fatalf("expected error for 500 response, got nil")
	}
	if !strings.Contains(err.Error(), "500") {
		t.Errorf("expected error to mention status 500, got: %v", err)
	}
}
