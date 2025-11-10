// RustMQ Admin API Client Example
//
// This example demonstrates how to use the RustMQ Admin API client for:
// - Cluster management (health, brokers, topics)
// - Security operations (CA management, certificate issuance, ACL management)
// - Audit log queries
//
// Run with: go run examples/admin_api_example.go

package main

import (
	"context"
	"crypto/tls"
	"fmt"
	"log"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

func main() {
	fmt.Println("=== RustMQ Admin API Client Example ===\n")

	// Create admin client configuration
	config := &rustmq.AdminClientConfig{
		TLSConfig: &tls.Config{
			InsecureSkipVerify: true, // For local testing only
		},
		Timeout: 30 * time.Second,
	}

	// Create admin client
	client, err := rustmq.NewAdminClient("http://localhost:8080", config)
	if err != nil {
		log.Fatalf("Failed to create admin client: %v", err)
	}
	fmt.Println("✓ Admin client created successfully\n")

	ctx := context.Background()

	// ==================== Health & Cluster Management ====================
	fmt.Println("--- Health & Cluster Management ---")

	// Check cluster health
	health, err := client.Health(ctx)
	if err != nil {
		fmt.Printf("✗ Health check failed: %v\n\n", err)
	} else {
		fmt.Println("✓ Health check:")
		fmt.Printf("  Status: %s\n", health.Status)
		fmt.Printf("  Version: %s\n", health.Version)
		fmt.Printf("  Uptime: %d seconds\n", health.UptimeSeconds)
		fmt.Printf("  Is Leader: %t\n", health.IsLeader)
		fmt.Printf("  Raft Term: %d\n\n", health.RaftTerm)
	}

	// Get cluster status
	clusterStatus, err := client.GetClusterStatus(ctx)
	if err != nil {
		fmt.Printf("✗ Cluster status failed: %v\n\n", err)
	} else {
		fmt.Println("✓ Cluster status:")
		fmt.Printf("  Brokers: %d\n", len(clusterStatus.Brokers))
		fmt.Printf("  Topics: %d\n", len(clusterStatus.Topics))
		if clusterStatus.Leader != nil {
			fmt.Printf("  Leader: %s\n", *clusterStatus.Leader)
		}
		fmt.Printf("  Term: %d\n", clusterStatus.Term)
		fmt.Printf("  Healthy: %t\n\n", clusterStatus.Healthy)

		for _, broker := range clusterStatus.Brokers {
			fmt.Printf("  Broker: %s (%s:%d)\n", broker.ID, broker.Host, broker.PortQuic)
			fmt.Printf("    Online: %t\n", broker.Online)
			fmt.Printf("    Rack: %s\n\n", broker.RackID)
		}
	}

	// List brokers
	brokers, err := client.ListBrokers(ctx)
	if err != nil {
		fmt.Printf("✗ List brokers failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Brokers (%d)\n", len(brokers))
		for _, broker := range brokers {
			status := "offline"
			if broker.Online {
				status = "online"
			}
			fmt.Printf("  - %s @ %s:%d (%s)\n",
				broker.ID, broker.Host, broker.PortQuic, status)
		}
		fmt.Println()
	}

	// ==================== Topic Management ====================
	fmt.Println("--- Topic Management ---")

	// List topics
	topics, err := client.ListTopics(ctx)
	if err != nil {
		fmt.Printf("✗ List topics failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Topics (%d)\n", len(topics))
		for _, topic := range topics {
			fmt.Printf("  - %s (partitions: %d, replication: %d)\n",
				topic.Name, topic.Partitions, topic.ReplicationFactor)
		}
		fmt.Println()
	}

	// Create a new topic
	createTopicReq := &rustmq.CreateTopicRequest{
		Name:              "admin-test-topic",
		Partitions:        3,
		ReplicationFactor: 2,
		RetentionMs:       uint64Ptr(86400000), // 24 hours
		SegmentBytes:      uint64Ptr(1073741824), // 1 GB
		CompressionType:   stringPtr("lz4"),
	}

	err = client.CreateTopic(ctx, createTopicReq)
	if err != nil {
		fmt.Printf("✗ Create topic failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Topic created successfully\n\n")
	}

	// Describe topic
	topicDetail, err := client.GetTopicDetail(ctx, "admin-test-topic")
	if err != nil {
		fmt.Printf("✗ Describe topic failed: %v\n\n", err)
	} else {
		fmt.Println("✓ Topic details:")
		fmt.Printf("  Name: %s\n", topicDetail.Name)
		fmt.Printf("  Partitions: %d\n", topicDetail.Partitions)
		fmt.Printf("  Replication Factor: %d\n", topicDetail.ReplicationFactor)
		fmt.Printf("  Created: %s\n", topicDetail.CreatedAt)
		fmt.Printf("  Partition Assignments: %d\n\n", len(topicDetail.PartitionAssignments))
	}

	// ==================== Security - CA Management ====================
	fmt.Println("--- CA Management ---")

	// Generate a new CA
	caReq := &rustmq.CaInitRequest{
		CommonName:   "RustMQ Admin Example CA",
		Organization: stringPtr("Example Org"),
		Country:      stringPtr("US"),
		ValidityDays: uint32Ptr(365),
		KeySize:      uint32Ptr(2048),
	}

	caInfo, err := client.InitCA(ctx, caReq)
	if err != nil {
		fmt.Printf("✗ Generate CA failed: %v\n\n", err)
	} else {
		fmt.Println("✓ CA generated:")
		fmt.Printf("  CA ID: %s\n", caInfo.CaID)
		fmt.Printf("  Common Name: %s\n", caInfo.CommonName)
		fmt.Printf("  Subject: %s\n", caInfo.Subject)
		fmt.Printf("  Valid From: %s\n", caInfo.NotBefore)
		fmt.Printf("  Valid Until: %s\n", caInfo.NotAfter)
		fmt.Printf("  Fingerprint: %s\n\n", caInfo.Fingerprint)
	}

	// Get CA info
	caInfoResult, err := client.GetCAInfo(ctx)
	if err != nil {
		fmt.Printf("✗ Get CA info failed: %v\n\n", err)
	} else {
		fmt.Println("✓ CA Information:")
		fmt.Printf("  CA ID: %s\n", caInfoResult.CaID)
		fmt.Printf("  Common Name: %s\n", caInfoResult.CommonName)
		fmt.Printf("  Valid: %s to %s\n\n", caInfoResult.NotBefore, caInfoResult.NotAfter)
	}

	// ==================== Security - Certificate Management ====================
	fmt.Println("--- Certificate Management ---")

	// Issue a certificate (need CA ID from previous step)
	certReq := &rustmq.IssueCertificateRequest{
		CaID:       caInfo.CaID,
		CommonName: "broker-1.example.com",
		SubjectAltNames: []string{
			"broker-1.local",
			"192.168.1.100",
		},
		Organization: stringPtr("Example Org"),
		Role:         stringPtr("broker"),
		ValidityDays: uint32Ptr(90),
		KeyUsage: []string{
			"digitalSignature",
			"keyEncipherment",
		},
		ExtendedKeyUsage: []string{
			"serverAuth",
			"clientAuth",
		},
	}

	cert, err := client.IssueCertificate(ctx, certReq)
	if err != nil {
		fmt.Printf("✗ Issue certificate failed: %v\n\n", err)
	} else {
		fmt.Println("✓ Certificate issued:")
		fmt.Printf("  Certificate ID: %s\n", cert.CertificateID)
		fmt.Printf("  Common Name: %s\n", cert.CommonName)
		fmt.Printf("  Serial: %s\n", cert.SerialNumber)
		fmt.Printf("  Valid From: %s\n", cert.NotBefore)
		fmt.Printf("  Valid Until: %s\n", cert.NotAfter)
		fmt.Printf("  Status: %s\n", cert.Status)
		if cert.Role != nil {
			fmt.Printf("  Role: %s\n", *cert.Role)
		}
		fmt.Println()
	}

	// List certificates
	certs, err := client.ListCertificates(ctx)
	if err != nil {
		fmt.Printf("✗ List certificates failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Certificates (%d)\n", len(certs))
		displayCount := len(certs)
		if displayCount > 5 {
			displayCount = 5
		}
		for i := 0; i < displayCount; i++ {
			c := certs[i]
			fmt.Printf("  - %s (%s)\n", c.CommonName, c.CertificateID)
			fmt.Printf("    Status: %s, Expires: %s\n", c.Status, c.NotAfter)
		}
		if len(certs) > 5 {
			fmt.Printf("  ... and %d more\n", len(certs)-5)
		}
		fmt.Println()
	}

	// Get certificate status (if we have a cert ID)
	if cert != nil {
		certStatus, err := client.GetCertificateStatus(ctx, cert.CertificateID)
		if err != nil {
			fmt.Printf("✗ Get certificate status failed: %v\n\n", err)
		} else {
			fmt.Printf("✓ Certificate Status for %s:\n", cert.CertificateID)
			fmt.Printf("  Status: %s\n", certStatus.Status)
			fmt.Printf("  Common Name: %s\n", certStatus.CommonName)
			fmt.Printf("  Not Before: %s\n", certStatus.NotBefore)
			fmt.Printf("  Not After: %s\n", certStatus.NotAfter)
			if certStatus.RevocationTime != nil {
				fmt.Printf("  Revoked At: %s\n", *certStatus.RevocationTime)
				if certStatus.RevocationReason != nil {
					fmt.Printf("  Revocation Reason: %s\n", *certStatus.RevocationReason)
				}
			}
			fmt.Println()
		}
	}

	// ==================== Security - ACL Management ====================
	fmt.Println("--- ACL Management ---")

	// Create ACL rule
	aclReq := &rustmq.CreateAclRuleRequest{
		Principal:       "user@example.com",
		ResourcePattern: "topic.events.*",
		ResourceType:    "topic",
		Operation:       "read",
		Effect:          "allow",
		Conditions: map[string]string{
			"source_ip": "192.168.1.0/24",
		},
	}

	aclRule, err := client.CreateACL(ctx, aclReq)
	if err != nil {
		fmt.Printf("✗ Create ACL rule failed: %v\n\n", err)
	} else {
		fmt.Println("✓ ACL rule created:")
		fmt.Printf("  Rule ID: %s\n", aclRule.RuleID)
		fmt.Printf("  Principal: %s\n", aclRule.Principal)
		fmt.Printf("  Resource: %s\n", aclRule.ResourcePattern)
		fmt.Printf("  Operation: %s\n", aclRule.Operation)
		fmt.Printf("  Effect: %s\n", aclRule.Effect)
		fmt.Printf("  Created: %s\n\n", aclRule.CreatedAt)
	}

	// List ACL rules
	aclRules, err := client.ListACLs(ctx, nil)
	if err != nil {
		fmt.Printf("✗ List ACL rules failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ ACL Rules (%d)\n", len(aclRules))
		displayCount := len(aclRules)
		if displayCount > 5 {
			displayCount = 5
		}
		for i := 0; i < displayCount; i++ {
			rule := aclRules[i]
			fmt.Printf("  - %s %s on %s\n",
				rule.Principal, rule.Operation, rule.ResourcePattern)
		}
		if len(aclRules) > 5 {
			fmt.Printf("  ... and %d more\n", len(aclRules)-5)
		}
		fmt.Println()
	}

	// Evaluate ACL permission
	evalReq := &rustmq.EvaluatePermissionRequest{
		Principal: "user@example.com",
		Resource:  "topic.events.payments",
		Operation: "read",
		Context: map[string]string{
			"source_ip": "192.168.1.100",
		},
	}

	evalResult, err := client.EvaluatePermission(ctx, evalReq)
	if err != nil {
		fmt.Printf("✗ ACL evaluation failed: %v\n\n", err)
	} else {
		fmt.Println("✓ ACL evaluation:")
		fmt.Printf("  Allowed: %t\n", evalResult.Allowed)
		fmt.Printf("  Effect: %s\n", evalResult.Effect)
		fmt.Printf("  Matched Rules: %v\n", evalResult.MatchedRules)
		fmt.Printf("  Evaluation Time: %d ns\n\n", evalResult.EvaluationTimeNs)
	}

	// ==================== Security - Audit Logs ====================
	fmt.Println("--- Audit Logs ---")

	// Query audit logs
	auditReq := &rustmq.AuditLogQuery{
		EventType: stringPtr("authentication"),
		Limit:     uint32Ptr(10),
		Offset:    uint32Ptr(0),
	}

	auditLogs, err := client.GetAuditLogs(ctx, auditReq)
	if err != nil {
		fmt.Printf("✗ Get audit logs failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Audit Logs (%d)\n", auditLogs.TotalCount)
		displayCount := len(auditLogs.Events)
		if displayCount > 5 {
			displayCount = 5
		}
		for i := 0; i < displayCount; i++ {
			entry := auditLogs.Events[i]
			principal := "N/A"
			if entry.Principal != nil {
				principal = *entry.Principal
			}
			fmt.Printf("  [%s %s] %s - %s\n",
				entry.Timestamp, entry.EventType, principal, entry.Result)
		}
		if len(auditLogs.Events) > 5 {
			fmt.Printf("  ... and %d more\n", len(auditLogs.Events)-5)
		}
		fmt.Println()
	}

	// Get audit trail for specific principal
	auditTrail, err := client.GetAuditTrail(ctx, "user@example.com")
	if err != nil {
		fmt.Printf("✗ Get audit trail failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Audit trail for user@example.com (%d total)\n", auditTrail.TotalCount)
		displayCount := len(auditTrail.Events)
		if displayCount > 3 {
			displayCount = 3
		}
		for i := 0; i < displayCount; i++ {
			entry := auditTrail.Events[i]
			operation := "N/A"
			if entry.Operation != nil {
				operation = *entry.Operation
			}
			resource := "N/A"
			if entry.Resource != nil {
				resource = *entry.Resource
			}
			fmt.Printf("  [%s %s] %s on %s\n",
				entry.Timestamp, entry.EventType, operation, resource)
		}
		if len(auditTrail.Events) > 3 {
			fmt.Printf("  ... and %d more\n", len(auditTrail.Events)-3)
		}
		fmt.Println()
	}

	// ==================== Cleanup ====================
	fmt.Println("--- Cleanup ---")

	// Delete test topic
	err = client.DeleteTopic(ctx, "admin-test-topic")
	if err != nil {
		fmt.Printf("✗ Delete topic failed: %v\n\n", err)
	} else {
		fmt.Printf("✓ Topic 'admin-test-topic' deleted successfully\n\n")
	}

	// Delete ACL rule (if we have a rule ID)
	if aclRule != nil {
		err := client.DeleteACL(ctx, aclRule.RuleID)
		if err != nil {
			fmt.Printf("✗ Delete ACL rule failed: %v\n\n", err)
		} else {
			fmt.Printf("✓ ACL rule %s deleted successfully\n\n", aclRule.RuleID)
		}
	}

	// Revoke certificate (if we have a cert ID)
	if cert != nil {
		err := client.RevokeCertificate(ctx, cert.SerialNumber, "testing")
		if err != nil {
			fmt.Printf("✗ Revoke certificate failed: %v\n\n", err)
		} else {
			fmt.Printf("✓ Certificate %s revoked successfully\n\n", cert.SerialNumber)
		}
	}

	fmt.Println("=== Example Complete ===")
}

// Helper functions for creating pointers
func stringPtr(s string) *string {
	return &s
}

func intPtr(i int) *int {
	return &i
}

func uint32Ptr(u uint32) *uint32 {
	return &u
}

func uint64Ptr(u uint64) *uint64 {
	return &u
}
