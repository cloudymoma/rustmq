package tests

import (
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// getTestClientConfig returns a client config optimized for tests (fast timeouts, no retries)
// This prevents tests from hanging when no broker is available
func getTestClientConfig() *rustmq.ClientConfig {
	config := rustmq.DefaultClientConfig()
	config.ConnectTimeout = 1 * time.Second
	config.RequestTimeout = 1 * time.Second
	config.RetryConfig.MaxRetries = 0
	return config
}
