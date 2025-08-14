package main

import (
	"context"
	"encoding/binary"
	"fmt"
	"log"
	"math/rand"
	"os"
	"os/signal"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// PerformanceMetrics tracks high-throughput performance
type PerformanceMetrics struct {
	startTime       time.Time
	messagesSent    int64
	messagesFailed  int64
	bytesSent       int64
	latencySum      int64 // nanoseconds
	latencyCount    int64
	latencyMin      int64
	latencyMax      int64
}

func (pm *PerformanceMetrics) recordMessage(latency time.Duration, bytes int, success bool) {
	if success {
		atomic.AddInt64(&pm.messagesSent, 1)
		atomic.AddInt64(&pm.bytesSent, int64(bytes))
	} else {
		atomic.AddInt64(&pm.messagesFailed, 1)
	}
	
	// Track latency
	latencyNs := latency.Nanoseconds()
	atomic.AddInt64(&pm.latencySum, latencyNs)
	atomic.AddInt64(&pm.latencyCount, 1)
	
	// Update min/max latency atomically
	for {
		currentMin := atomic.LoadInt64(&pm.latencyMin)
		if currentMin == 0 || latencyNs < currentMin {
			if atomic.CompareAndSwapInt64(&pm.latencyMin, currentMin, latencyNs) {
				break
			}
		} else {
			break
		}
	}
	
	for {
		currentMax := atomic.LoadInt64(&pm.latencyMax)
		if latencyNs > currentMax {
			if atomic.CompareAndSwapInt64(&pm.latencyMax, currentMax, latencyNs) {
				break
			}
		} else {
			break
		}
	}
}

func (pm *PerformanceMetrics) report() {
	duration := time.Since(pm.startTime)
	sent := atomic.LoadInt64(&pm.messagesSent)
	failed := atomic.LoadInt64(&pm.messagesFailed)
	bytes := atomic.LoadInt64(&pm.bytesSent)
	latencySum := atomic.LoadInt64(&pm.latencySum)
	latencyCount := atomic.LoadInt64(&pm.latencyCount)
	latencyMin := atomic.LoadInt64(&pm.latencyMin)
	latencyMax := atomic.LoadInt64(&pm.latencyMax)
	
	throughputMsgs := float64(sent) / duration.Seconds()
	throughputMB := float64(bytes) / (1024 * 1024) / duration.Seconds()
	
	avgLatency := time.Duration(0)
	if latencyCount > 0 {
		avgLatency = time.Duration(latencySum / latencyCount)
	}
	
	successRate := float64(0)
	if sent+failed > 0 {
		successRate = float64(sent) / float64(sent+failed) * 100
	}
	
	fmt.Printf("\n🚀 High-Throughput Performance Report:\n")
	fmt.Printf("   Duration: %v\n", duration.Round(time.Millisecond))
	fmt.Printf("   Messages: %d sent, %d failed (%.2f%% success)\n", sent, failed, successRate)
	fmt.Printf("   Throughput: %.0f msgs/sec, %.2f MB/sec\n", throughputMsgs, throughputMB)
	fmt.Printf("   Latency: avg=%v, min=%v, max=%v\n", 
		avgLatency.Round(time.Microsecond),
		time.Duration(latencyMin).Round(time.Microsecond),
		time.Duration(latencyMax).Round(time.Microsecond))
	fmt.Printf("   Data: %.2f MB total\n", float64(bytes)/(1024*1024))
}

func main() {
	// High-throughput configuration
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9092"},
		ClientID:          "high-throughput-producer",
		ConnectTimeout:    15 * time.Second,
		RequestTimeout:    2 * time.Second, // Fast timeouts for high throughput
		EnableTLS:         false,
		MaxConnections:    50, // Many connections for high throughput
		KeepAliveInterval: 30 * time.Second,
		RetryConfig: &rustmq.RetryConfig{
			MaxRetries: 2, // Fewer retries for speed
			BaseDelay:  50 * time.Millisecond,
			MaxDelay:   2 * time.Second,
			Multiplier: 1.5,
			Jitter:     true,
		},
		Compression: &rustmq.CompressionConfig{
			Enabled:   true,
			Algorithm: rustmq.CompressionLZ4, // Fastest compression
			Level:     1, // Fastest compression level
			MinSize:   100,
		},
	}

	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	fmt.Println("🔥 High-throughput RustMQ producer ready")

	// Optimized producer configuration for maximum throughput
	producerConfig := &rustmq.ProducerConfig{
		ProducerID:     "high-throughput-001",
		BatchSize:      1000, // Large batches for efficiency
		BatchTimeout:   1 * time.Millisecond, // Very fast batching
		MaxMessageSize: 1024 * 1024, // 1MB max
		AckLevel:       rustmq.AckLeader, // Balance between speed and durability
		Idempotent:     false, // Disable for maximum speed
		Compression: &rustmq.CompressionConfig{
			Enabled:   true,
			Algorithm: rustmq.CompressionLZ4,
			Level:     1,
			MinSize:   50,
		},
		DefaultProps: map[string]string{
			"producer-type": "high-throughput",
			"benchmark":     "true",
		},
	}

	producer, err := client.CreateProducer("benchmark.throughput", producerConfig)
	if err != nil {
		log.Fatalf("Failed to create producer: %v", err)
	}
	defer producer.Close()

	// Performance tracking
	metrics := &PerformanceMetrics{startTime: time.Now()}
	
	// Graceful shutdown
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	// Continuous metrics reporting
	go func() {
		ticker := time.NewTicker(5 * time.Second)
		defer ticker.Stop()
		
		for {
			select {
			case <-ticker.C:
				metrics.report()
			case <-ctx.Done():
				return
			}
		}
	}()

	fmt.Println("🚀 Starting high-throughput benchmark...")
	fmt.Println("Press Ctrl+C to stop and see final results")

	// High-throughput producer pattern
	workers := 20 // Many concurrent workers
	messageSize := 1024 // 1KB messages
	
	var wg sync.WaitGroup

	// Start producer workers
	for workerID := 0; workerID < workers; workerID++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			
			// Pre-allocate message payload for efficiency
			payload := make([]byte, messageSize)
			
			messageCounter := 0
			for {
				select {
				case <-ctx.Done():
					return
				default:
					sendHighThroughputMessage(ctx, producer, metrics, id, messageCounter, payload)
					messageCounter++
					
					// No delay - maximum speed!
				}
			}
		}(workerID)
	}

	// Wait for shutdown signal
	<-sigCh
	fmt.Println("\n🛑 Stopping high-throughput benchmark...")
	cancel()

	// Wait for workers to stop
	fmt.Println("⏳ Waiting for workers to finish...")
	wg.Wait()

	// Final flush
	fmt.Println("🔄 Flushing remaining messages...")
	flushCtx, flushCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer flushCancel()
	
	if err := producer.Flush(flushCtx); err != nil {
		fmt.Printf("⚠️ Flush error: %v\n", err)
	} else {
		fmt.Println("✅ Final flush completed")
	}

	// Final performance report
	fmt.Println("\n" + strings.Repeat("=", 60))
	fmt.Println("📈 FINAL BENCHMARK RESULTS")
	fmt.Println(strings.Repeat("=", 60))
	metrics.report()
	
	// Producer metrics
	pMetrics := producer.Metrics()
	fmt.Printf("\n📦 Producer Internal Metrics:\n")
	fmt.Printf("   Batches: %d (avg %.1f msgs/batch)\n", pMetrics.BatchesSent, pMetrics.AverageBatchSize)
	fmt.Printf("   Producer Bytes: %d\n", pMetrics.BytesSent)
	fmt.Printf("   Last Send: %v\n", pMetrics.LastSendTime.Format("15:04:05.000"))
	
	// Connection stats
	stats := client.Stats()
	fmt.Printf("\n🔗 Connection Statistics:\n")
	fmt.Printf("   Bytes Sent: %d, Received: %d\n", stats.BytesSent, stats.BytesReceived)
	fmt.Printf("   Requests: %d, Responses: %d\n", stats.RequestsSent, stats.ResponsesReceived)
	fmt.Printf("   Errors: %d\n", stats.Errors)

	fmt.Println("\n🏁 High-throughput benchmark completed!")
}

func sendHighThroughputMessage(ctx context.Context, producer *rustmq.Producer, metrics *PerformanceMetrics, workerID, msgNum int, payload []byte) {
	// Generate efficient message payload
	binary.LittleEndian.PutUint32(payload[0:4], uint32(workerID))
	binary.LittleEndian.PutUint32(payload[4:8], uint32(msgNum))
	binary.LittleEndian.PutUint64(payload[8:16], uint64(time.Now().UnixNano()))
	
	// Fill rest with pseudo-random data for realistic compression testing
	seed := int64(workerID)*1000000 + int64(msgNum)
	r := rand.New(rand.NewSource(seed))
	for i := 16; i < len(payload); i += 8 {
		if i+8 <= len(payload) {
			binary.LittleEndian.PutUint64(payload[i:i+8], r.Uint64())
		}
	}

	// Build message efficiently
	msgID := fmt.Sprintf("msg-%d-%d", workerID, msgNum)
	partitionKey := fmt.Sprintf("worker-%d", workerID) // Distribute across partitions
	
	message := rustmq.NewMessage().
		Topic("benchmark.throughput").
		ID(msgID).
		KeyString(partitionKey).
		Payload(payload).
		Header("worker-id", fmt.Sprintf("%d", workerID)).
		Header("msg-num", fmt.Sprintf("%d", msgNum)).
		Build()

	// Send asynchronously for maximum throughput
	start := time.Now()
	err := producer.SendAsync(ctx, message, func(result *rustmq.MessageResult, err error) {
		latency := time.Since(start)
		success := err == nil
		metrics.recordMessage(latency, len(payload), success)
		
		// Log errors (successes are too frequent to log)
		if !success {
			fmt.Printf("❌ Worker %d msg %d failed: %v\n", workerID, msgNum, err)
		}
	})
	
	if err != nil {
		// Immediate send error (not callback error)
		metrics.recordMessage(time.Since(start), len(payload), false)
	}
}