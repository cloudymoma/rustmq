use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::{info, error, debug};
use sysinfo::{System, Disks, Networks};
use crate::metrics::Metrics;
use crate::broker::broker::BrokerCore;
use warp::Filter;
use prometheus::Encoder;
use crate::proto::controller::broker_management_service_client::BrokerManagementServiceClient;
use crate::proto::controller::BrokerHeartbeatRequest;

pub struct MetricsCollector {
    broker_core: Arc<BrokerCore>,
    metrics: Arc<Metrics>,
    system: System,
    last_network_in: u64,
    last_network_out: u64,
    last_message_count: u64,
    controller_endpoint: String,
    broker_id: String,
}

impl MetricsCollector {
    pub fn new(
        broker_core: Arc<BrokerCore>,
        metrics: Arc<Metrics>,
        controller_endpoint: String,
        broker_id: String,
    ) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let networks = Networks::new_with_refreshed_list();
        let mut network_in = 0u64;
        let mut network_out = 0u64;
        for (_name, network) in &networks {
            network_in += network.received();
            network_out += network.transmitted();
        }

        let last_message_count = broker_core.get_total_messages_processed();

        Self {
            broker_core,
            metrics,
            system,
            last_network_in: network_in,
            last_network_out: network_out,
            last_message_count,
            controller_endpoint,
            broker_id,
        }
    }

    pub fn start(mut self, admin_port: u16) -> tokio::task::JoinHandle<()> {
        let collector_metrics = self.metrics.clone();

        let metrics_route = warp::path("metrics")
            .and(warp::get())
            .and(warp::any().map(move || collector_metrics.clone()))
            .map(|metrics: Arc<Metrics>| {
                let encoder = prometheus::TextEncoder::new();
                let metric_families = metrics.registry.gather();
                let mut buffer = vec![];
                encoder.encode(&metric_families, &mut buffer).unwrap();
                warp::reply::with_header(
                    String::from_utf8(buffer).unwrap(),
                    "Content-Type",
                    encoder.format_type(),
                )
            });

        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(10));
            info!(
                "MetricsCollector started with 10s interval, serving /metrics on port {}",
                admin_port
            );

            let server = warp::serve(metrics_route).run(([0, 0, 0, 0], admin_port));

            tokio::select! {
                _ = server => {
                    error!("Metrics server stopped unexpectedly");
                }
                _ = async {
                    loop {
                        tick.tick().await;
                        self.collect_and_report().await;
                    }
                } => {
                    error!("Metrics collection loop stopped unexpectedly");
                }
            }
        })
    }

    async fn collect_and_report(&mut self) {
        self.system.refresh_all();

        let cpu_usage = self.system.global_cpu_info().cpu_usage() as f64;
        self.metrics.cpu_usage.set(cpu_usage / 100.0);

        let memory_used = self.system.used_memory() as f64;
        let memory_total = self.system.total_memory() as f64;
        let mem_ratio = if memory_total > 0.0 {
            memory_used / memory_total
        } else {
            0.0
        };
        self.metrics.memory_usage.set(mem_ratio);

        let disks = Disks::new_with_refreshed_list();
        let mut disk_total: u64 = 0;
        let mut disk_used: u64 = 0;
        for disk in &disks {
            disk_total += disk.total_space();
            disk_used += disk.total_space() - disk.available_space();
        }
        let disk_ratio = if disk_total > 0 {
            disk_used as f64 / disk_total as f64
        } else {
            0.0
        };
        self.metrics.disk_usage.set(disk_ratio);

        let networks = Networks::new_with_refreshed_list();
        let mut network_in: u64 = 0;
        let mut network_out: u64 = 0;
        for (_name, network) in &networks {
            network_in += network.received();
            network_out += network.transmitted();
        }

        let in_delta = network_in.saturating_sub(self.last_network_in);
        let out_delta = network_out.saturating_sub(self.last_network_out);

        self.metrics.network_bytes.with_label_values(&["rx"]).inc_by(in_delta as f64);
        self.metrics.network_bytes.with_label_values(&["tx"]).inc_by(out_delta as f64);

        self.last_network_in = network_in;
        self.last_network_out = network_out;

        let (total_partitions, leader_partitions) = self.broker_core.get_partition_counts().await;
        self.metrics.partition_count.with_label_values(&["leader"]).set(leader_partitions as f64);
        self.metrics
            .partition_count
            .with_label_values(&["follower"])
            .set((total_partitions - leader_partitions) as f64);

        let current_message_count = self.broker_core.get_total_messages_processed();
        let message_delta = current_message_count.saturating_sub(self.last_message_count);
        let message_rate = message_delta / 10; // per-second rate (10s interval)
        self.last_message_count = current_message_count;

        if message_delta > 0 {
            self.metrics.message_rate.inc_by(message_delta as f64);
        }

        let rx_per_sec = (in_delta as f64 / 10.0) as u64;
        let tx_per_sec = (out_delta as f64 / 10.0) as u64;

        let proto_metrics = crate::proto::controller::LoadMetrics {
            cpu_usage: cpu_usage / 100.0,
            memory_usage: mem_ratio,
            disk_usage: disk_ratio,
            network_tx_bytes_sec: tx_per_sec,
            network_rx_bytes_sec: rx_per_sec,
            partition_count: total_partitions as u32,
            leader_partition_count: leader_partitions as u32,
            message_rate,
            consumer_lag_total: 0,
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        };

        let request = BrokerHeartbeatRequest {
            broker_id: self.broker_id.clone(),
            metrics: Some(proto_metrics),
        };

        let endpoint = format!("http://{}", self.controller_endpoint);
        match BrokerManagementServiceClient::connect(endpoint).await {
            Ok(mut client) => match client.broker_heartbeat(request).await {
                Ok(response) => {
                    let age = response.into_inner().heartbeat_age_seconds;
                    self.metrics.heartbeat_age.set(age as f64);
                    debug!("Heartbeat sent (age={}s, msg_rate={}/s)", age, message_rate);
                }
                Err(e) => debug!("Failed to send heartbeat: {}", e),
            },
            Err(e) => debug!("Failed to connect to controller for heartbeat: {}", e),
        }
    }
}
