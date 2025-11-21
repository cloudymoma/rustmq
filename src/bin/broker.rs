//! RustMQ Broker CLI - Thin wrapper around Broker abstraction
//!
//! This binary provides a command-line interface for running the RustMQ broker.
//! All broker logic is implemented in the Broker struct in the broker module.

use rustmq::{Config, Result, broker::Broker};
use std::env;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Install panic hook for production safety
    rustmq::panic_handler::install_panic_hook();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let default_config = "config/broker.toml".to_string();
    let config_path = args.get(2).unwrap_or(&default_config);

    info!("Starting RustMQ Broker");
    info!("Loading configuration from: {}", config_path);

    // Load configuration
    let config = match Config::from_file(config_path) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Create and start broker
    let mut broker = Broker::from_config(config).await?;
    broker.start().await?;

    // Wait for shutdown signal
    signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    info!("Received shutdown signal (Ctrl+C)");

    // Gracefully stop the broker
    broker.stop().await?;

    Ok(())
}
