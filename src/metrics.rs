use prometheus::{Counter, CounterVec, Gauge, GaugeVec, Histogram, Registry};
use std::sync::Arc;

pub struct Metrics {
    pub messages_produced: Counter,
    pub messages_consumed: Counter,
    pub produce_latency: Histogram,
    pub consume_latency: Histogram,
    pub active_connections: Gauge,
    pub cpu_usage: Gauge,
    pub memory_usage: Gauge,
    pub disk_usage: Gauge,
    pub partition_count: GaugeVec,
    pub message_rate: Counter,
    pub consumer_lag_total: Gauge,
    pub network_bytes: CounterVec,
    pub heartbeat_age: Gauge,
    pub partition_high_watermark: GaugeVec,
    pub group_committed_offset: GaugeVec,
    pub consumer_group_members: GaugeVec,
    pub consumer_group_rebalances_total: CounterVec,
    pub registry: Registry,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();

        let messages_produced = Counter::new(
            "messages_produced_total",
            "Total number of messages produced",
        )
        .expect("metric");
        let messages_consumed = Counter::new(
            "messages_consumed_total",
            "Total number of messages consumed",
        )
        .expect("metric");
        let produce_latency = Histogram::with_opts(prometheus::HistogramOpts::new(
            "produce_latency_seconds",
            "Produce request latency",
        ))
        .expect("metric");
        let consume_latency = Histogram::with_opts(prometheus::HistogramOpts::new(
            "consume_latency_seconds",
            "Consume request latency",
        ))
        .expect("metric");
        let active_connections =
            Gauge::new("active_connections", "Number of active connections").expect("metric");

        let cpu_usage =
            Gauge::new("rustmq_broker_cpu_usage", "Broker CPU usage (0.0-1.0)").expect("metric");
        let memory_usage = Gauge::new(
            "rustmq_broker_memory_usage",
            "Broker memory usage (0.0-1.0)",
        )
        .expect("metric");
        let disk_usage =
            Gauge::new("rustmq_broker_disk_usage", "Broker disk usage (0.0-1.0)").expect("metric");
        let partition_count = GaugeVec::new(
            prometheus::Opts::new("rustmq_broker_partition_count", "Broker partition count"),
            &["role"],
        )
        .expect("metric");
        let message_rate =
            Counter::new("rustmq_broker_message_rate", "Total messages processed").expect("metric");
        let consumer_lag_total =
            Gauge::new("rustmq_broker_consumer_lag_total", "Total consumer lag").expect("metric");
        let network_bytes = CounterVec::new(
            prometheus::Opts::new("rustmq_broker_network_bytes_total", "Total network bytes"),
            &["direction"],
        )
        .expect("metric");
        let heartbeat_age = Gauge::new(
            "rustmq_broker_heartbeat_age_seconds",
            "Seconds since last heartbeat acknowledged by controller",
        )
        .expect("metric");

        let partition_high_watermark = GaugeVec::new(
            prometheus::Opts::new(
                "rustmq_partition_high_watermark",
                "Partition high watermark",
            ),
            &["topic", "partition"],
        )
        .expect("metric");

        let group_committed_offset = GaugeVec::new(
            prometheus::Opts::new(
                "rustmq_group_committed_offset",
                "Consumer group committed offset",
            ),
            &["group", "topic", "partition"],
        )
        .expect("metric");

        let consumer_group_members = GaugeVec::new(
            prometheus::Opts::new(
                "rustmq_consumer_group_members",
                "Active consumer group members",
            ),
            &["group"],
        )
        .expect("metric");

        let consumer_group_rebalances_total = CounterVec::new(
            prometheus::Opts::new(
                "rustmq_consumer_group_rebalances_total",
                "Total consumer group rebalances",
            ),
            &["group"],
        )
        .expect("metric");

        registry
            .register(Box::new(messages_produced.clone()))
            .unwrap();
        registry
            .register(Box::new(messages_consumed.clone()))
            .unwrap();
        registry
            .register(Box::new(produce_latency.clone()))
            .unwrap();
        registry
            .register(Box::new(consume_latency.clone()))
            .unwrap();
        registry
            .register(Box::new(active_connections.clone()))
            .unwrap();
        registry.register(Box::new(cpu_usage.clone())).unwrap();
        registry.register(Box::new(memory_usage.clone())).unwrap();
        registry.register(Box::new(disk_usage.clone())).unwrap();
        registry
            .register(Box::new(partition_count.clone()))
            .unwrap();
        registry.register(Box::new(message_rate.clone())).unwrap();
        registry
            .register(Box::new(consumer_lag_total.clone()))
            .unwrap();
        registry.register(Box::new(network_bytes.clone())).unwrap();
        registry.register(Box::new(heartbeat_age.clone())).unwrap();
        registry
            .register(Box::new(partition_high_watermark.clone()))
            .unwrap();
        registry
            .register(Box::new(group_committed_offset.clone()))
            .unwrap();
        registry
            .register(Box::new(consumer_group_members.clone()))
            .unwrap();
        registry
            .register(Box::new(consumer_group_rebalances_total.clone()))
            .unwrap();

        Arc::new(Self {
            messages_produced,
            messages_consumed,
            produce_latency,
            consume_latency,
            active_connections,
            cpu_usage,
            memory_usage,
            disk_usage,
            partition_count,
            message_rate,
            consumer_lag_total,
            network_bytes,
            heartbeat_age,
            partition_high_watermark,
            group_committed_offset,
            consumer_group_members,
            consumer_group_rebalances_total,
            registry,
        })
    }
}
