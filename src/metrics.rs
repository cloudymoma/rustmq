use prometheus::{Counter, Gauge, Histogram, Registry};
use std::sync::Arc;

pub struct Metrics {
    pub messages_produced: Counter,
    pub messages_consumed: Counter,
    pub produce_latency: Histogram,
    pub consume_latency: Histogram,
    pub active_connections: Gauge,
    pub registry: Registry,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();

        let messages_produced = Counter::new(
            "messages_produced_total",
            "Total number of messages produced",
        )
        .expect("Failed to create messages_produced counter");

        let messages_consumed = Counter::new(
            "messages_consumed_total",
            "Total number of messages consumed",
        )
        .expect("Failed to create messages_consumed counter");

        let produce_latency = Histogram::with_opts(prometheus::HistogramOpts::new(
            "produce_latency_seconds",
            "Produce request latency",
        ))
        .expect("Failed to create produce_latency histogram");

        let consume_latency = Histogram::with_opts(prometheus::HistogramOpts::new(
            "consume_latency_seconds",
            "Consume request latency",
        ))
        .expect("Failed to create consume_latency histogram");

        let active_connections = Gauge::new("active_connections", "Number of active connections")
            .expect("Failed to create active_connections gauge");

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

        Arc::new(Self {
            messages_produced,
            messages_consumed,
            produce_latency,
            consume_latency,
            active_connections,
            registry,
        })
    }
}
