package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"

	"github.com/rustmq/rustmq/sdk/go/rustmq"
)

// OrderEvent represents a sample business event
type OrderEvent struct {
	OrderID      string    `json:"order_id"`
	CustomerID   string    `json:"customer_id"`
	ProductID    string    `json:"product_id"`
	Quantity     int       `json:"quantity"`
	Price        float64   `json:"price"`
	Status       string    `json:"status"`
	CreatedAt    time.Time `json:"created_at"`
	Region       string    `json:"region"`
}

// MetricsCollector tracks production metrics
type MetricsCollector struct {
	totalSent      int64
	totalFailed    int64
	totalLatency   time.Duration
	minLatency     time.Duration
	maxLatency     time.Duration
	mutex          sync.RWMutex
}

func (m *MetricsCollector) recordSend(latency time.Duration, success bool) {
	m.mutex.Lock()
	defer m.mutex.Unlock()
	
	if success {
		m.totalSent++
	} else {
		m.totalFailed++
	}
	
	m.totalLatency += latency
	if m.minLatency == 0 || latency < m.minLatency {
		m.minLatency = latency
	}
	if latency > m.maxLatency {
		m.maxLatency = latency
	}
}

func (m *MetricsCollector) report() (int64, int64, time.Duration, time.Duration, time.Duration) {
	m.mutex.RLock()
	defer m.mutex.RUnlock()
	
	avgLatency := time.Duration(0)
	if m.totalSent > 0 {
		avgLatency = m.totalLatency / time.Duration(m.totalSent)
	}
	
	return m.totalSent, m.totalFailed, avgLatency, m.minLatency, m.maxLatency
}

func main() {
	// Production configuration
	config := &rustmq.ClientConfig{
		Brokers:           []string{"localhost:9092", "localhost:9093", "localhost:9094"},
		ClientID:          "production-order-producer",
		ConnectTimeout:    30 * time.Second,
		RequestTimeout:    10 * time.Second,
		EnableTLS:         false, // Set to true for production with proper certificates
		MaxConnections:    20,
		KeepAliveInterval: 30 * time.Second,
		RetryConfig: &rustmq.RetryConfig{
			MaxRetries: 5,
			BaseDelay:  200 * time.Millisecond,
			MaxDelay:   30 * time.Second,
			Multiplier: 2.0,
			Jitter:     true,
		},
		Compression: &rustmq.CompressionConfig{
			Enabled:   true,
			Algorithm: rustmq.CompressionGzip,
			Level:     6,
			MinSize:   512, // Only compress messages > 512 bytes
		},
	}

	// Create client with production settings
	client, err := rustmq.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create RustMQ client: %v", err)
	}
	defer client.Close()

	fmt.Println("🚀 Production RustMQ client connected")
	
	// Health check before starting
	if err := client.HealthCheck(); err != nil {
		log.Fatalf("Health check failed: %v", err)
	}
	fmt.Println("✅ Health check passed")

	// Production producer configuration
	producerConfig := &rustmq.ProducerConfig{
		ProducerID:     "order-events-producer-001",
		BatchSize:      50,  // Optimize for throughput
		BatchTimeout:   10 * time.Millisecond, // Low latency
		MaxMessageSize: 10 * 1024 * 1024, // 10MB max
		AckLevel:       rustmq.AckAll, // Strong consistency
		Idempotent:     true, // Prevent duplicates
		Compression: &rustmq.CompressionConfig{
			Enabled:   true,
			Algorithm: rustmq.CompressionGzip,
			Level:     6,
			MinSize:   256,
		},
		DefaultProps: map[string]string{
			"producer":    "order-service",
			"version":     "v1.2.3",
			"environment": "production",
			"region":      "us-central1",
		},
	}

	// Create producer for order events
	producer, err := client.CreateProducer("order.events", producerConfig)
	if err != nil {
		log.Fatalf("Failed to create producer: %v", err)
	}
	defer producer.Close()

	fmt.Printf("📦 Producer created: %s\n", producer.ID())

	// Metrics collection
	metrics := &MetricsCollector{}
	
	// Graceful shutdown handling
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Setup signal handling for graceful shutdown
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	// Start metrics reporting goroutine
	go func() {
		ticker := time.NewTicker(10 * time.Second)
		defer ticker.Stop()
		
		for {
			select {
			case <-ticker.C:
				reportMetrics(producer, metrics)
			case <-ctx.Done():
				return
			}
		}
	}()

	// Simulate production workload
	fmt.Println("🏭 Starting production message simulation...")
	
	var wg sync.WaitGroup
	workers := 5 // Concurrent producers
	messagesPerWorker := 200

	for workerID := 0; workerID < workers; workerID++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			
			for i := 0; i < messagesPerWorker; i++ {
				select {
				case <-ctx.Done():
					return
				default:
					sendOrderEvent(ctx, producer, metrics, id, i)
					
					// Realistic production rate - 100 msgs/sec per worker
					time.Sleep(10 * time.Millisecond)
				}
			}
		}(workerID)
	}

	// Wait for shutdown signal
	go func() {
		<-sigCh
		fmt.Println("\n🛑 Received shutdown signal, gracefully stopping...")
		cancel()
	}()

	// Wait for all workers to complete or shutdown
	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		fmt.Println("✅ All workers completed")
	case <-ctx.Done():
		fmt.Println("⏹️ Shutdown initiated, waiting for workers...")
		wg.Wait()
	}

	// Final flush and metrics
	fmt.Println("🔄 Flushing remaining messages...")
	flushCtx, flushCancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer flushCancel()
	
	if err := producer.Flush(flushCtx); err != nil {
		fmt.Printf("⚠️ Flush error: %v\n", err)
	} else {
		fmt.Println("✅ All messages flushed successfully")
	}

	// Final metrics report
	fmt.Println("\n📊 Final Production Metrics:")
	reportMetrics(producer, metrics)
	
	// Connection statistics
	stats := client.Stats()
	fmt.Printf("🔗 Connection Stats: Sent=%d bytes, Received=%d bytes, Errors=%d\n",
		stats.BytesSent, stats.BytesReceived, stats.Errors)

	fmt.Println("🏁 Production producer demo completed")
}

func sendOrderEvent(ctx context.Context, producer *rustmq.Producer, metrics *MetricsCollector, workerID, orderNum int) {
	orderID := fmt.Sprintf("ORD-%d-%06d", workerID, orderNum)
	
	// Create realistic order event
	order := OrderEvent{
		OrderID:    orderID,
		CustomerID: fmt.Sprintf("CUST-%06d", (workerID*1000)+orderNum),
		ProductID:  fmt.Sprintf("PROD-%03d", orderNum%100),
		Quantity:   (orderNum % 5) + 1,
		Price:      float64((orderNum%100)+10) * 9.99,
		Status:     []string{"pending", "confirmed", "processing", "shipped"}[orderNum%4],
		CreatedAt:  time.Now(),
		Region:     []string{"us-east-1", "us-west-2", "eu-west-1", "ap-south-1"}[orderNum%4],
	}

	// Build message with proper partitioning and headers
	message := rustmq.NewMessage().
		Topic("order.events").
		KeyString(order.CustomerID). // Partition by customer for ordering
		PayloadJSON(order).
		Header("event-type", "order.created").
		Header("order-id", order.OrderID).
		Header("customer-id", order.CustomerID).
		Header("region", order.Region).
		Header("priority", determinePriority(order)).
		Header("schema-version", "1.0").
		Header("correlation-id", fmt.Sprintf("corr-%s", orderID)).
		Build()

	// Send with timeout and metrics
	sendCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	
	start := time.Now()
	result, err := producer.Send(sendCtx, message)
	latency := time.Since(start)
	
	if err != nil {
		metrics.recordSend(latency, false)
		fmt.Printf("❌ Worker %d failed to send order %s: %v\n", workerID, orderID, err)
	} else {
		metrics.recordSend(latency, true)
		if orderNum%50 == 0 { // Log every 50th success for monitoring
			fmt.Printf("✅ Worker %d sent order %s: offset=%d, partition=%d, latency=%v\n",
				workerID, orderID, result.Offset, result.Partition, latency)
		}
	}
}

func determinePriority(order OrderEvent) string {
	if order.Price > 500 {
		return "high"
	} else if order.Price > 100 {
		return "medium"
	}
	return "low"
}

func reportMetrics(producer *rustmq.Producer, appMetrics *MetricsCollector) {
	// Producer metrics
	pMetrics := producer.Metrics()
	
	// Application metrics
	sent, failed, avgLatency, minLatency, maxLatency := appMetrics.report()
	
	// Calculate success rate
	successRate := float64(0)
	if sent+failed > 0 {
		successRate = float64(sent) / float64(sent+failed) * 100
	}
	
	fmt.Printf("📊 Metrics Report:\n")
	fmt.Printf("   Messages: Sent=%d, Failed=%d (%.2f%% success)\n", sent, failed, successRate)
	fmt.Printf("   Latency: Avg=%v, Min=%v, Max=%v\n", avgLatency, minLatency, maxLatency)
	fmt.Printf("   Producer: Batches=%d, AvgBatchSize=%.1f, BytesSent=%d\n",
		pMetrics.BatchesSent, pMetrics.AverageBatchSize, pMetrics.BytesSent)
	fmt.Printf("   Last Send: %v\n", pMetrics.LastSendTime.Format("15:04:05"))
}