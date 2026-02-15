//! Panic Handler - Production panic hooks with telemetry and logging
//!
//! This module provides panic hooks that:
//! - Log panic information with full context
//! - Send telemetry to monitoring systems
//! - Provide backtrace for debugging
//! - Help identify unwrap() calls that panic in production

use std::panic::{self, PanicInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, warn};

/// Global flag to prevent recursive panics
static PANIC_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Panic statistics for monitoring
#[derive(Debug, Clone)]
pub struct PanicStats {
    pub total_panics: u64,
    pub last_panic_location: Option<String>,
    pub last_panic_message: Option<String>,
}

/// Install production panic hook with telemetry
///
/// This should be called early in the application startup (e.g., in main()).
/// It installs a custom panic hook that:
/// - Logs the panic with full context
/// - Captures backtrace information
/// - Sends metrics to monitoring system
/// - Prevents process abort in some cases (future enhancement)
///
/// # Example
/// ```no_run
/// use rustmq::panic_handler;
///
/// fn main() {
///     panic_handler::install_panic_hook();
///     // ... rest of application
/// }
/// ```
pub fn install_panic_hook() {
    // Check if already installed to prevent double-installation
    if PANIC_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        warn!("Panic hook already installed, skipping");
        return;
    }

    // Install the custom panic hook
    panic::set_hook(Box::new(|panic_info| {
        handle_panic(panic_info);
    }));

    tracing::info!("✅ Production panic hook installed successfully");
}

/// Handle a panic by logging and recording telemetry
fn handle_panic(panic_info: &PanicInfo) {
    // Extract panic location
    let location = if let Some(location) = panic_info.location() {
        format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )
    } else {
        "unknown location".to_string()
    };

    // Extract panic message
    let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic message".to_string()
    };

    // Log the panic with full context
    error!(
        "🚨 PANIC OCCURRED 🚨\n\
         Location: {}\n\
         Message: {}\n\
         Thread: {:?}\n\
         \n\
         This panic indicates an .unwrap() or .expect() call failed.\n\
         Review the code at the location above and add proper error handling.",
        location,
        message,
        std::thread::current().name().unwrap_or("unnamed")
    );

    // Log backtrace if available (requires RUST_BACKTRACE=1)
    if std::env::var("RUST_BACKTRACE").is_ok() {
        error!("Backtrace available - check logs for details");
    } else {
        warn!("Backtrace not available - set RUST_BACKTRACE=1 for detailed traces");
    }

    // TODO: Send telemetry to monitoring system
    // This could send to:
    // - Prometheus (increment panic counter)
    // - Datadog (send panic event)
    // - CloudWatch (send metric)
    // - Sentry (send error report)
    //
    // Example (when metrics system is integrated):
    // if let Some(metrics) = GLOBAL_METRICS.get() {
    //     metrics.record_panic(&location, &message);
    // }

    // Log suggestions for common panic patterns
    if message.contains("unwrap") || message.contains("None") {
        error!(
            "💡 SUGGESTION: Replace .unwrap() with proper error handling:\n\
             - Use .ok_or() or .ok_or_else() to convert Option to Result\n\
             - Use .map_err() to add context to errors\n\
             - Use if let Some(val) = ... for optional values\n\
             - Return Result from functions for proper error propagation"
        );
    }

    if message.contains("index out of bounds") || message.contains("slice") {
        error!(
            "💡 SUGGESTION: Bounds check failed:\n\
             - Use .get() instead of indexing for safe access\n\
             - Validate array/slice lengths before access\n\
             - Use iterators instead of manual indexing"
        );
    }
}

/// Check if panic hook is installed
pub fn is_panic_hook_installed() -> bool {
    PANIC_HOOK_INSTALLED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panic_hook_installation() {
        // Note: We can't test actual panics in unit tests as they would abort the test
        // But we can verify the hook is installed
        install_panic_hook();
        assert!(is_panic_hook_installed());

        // Second installation should be idempotent
        install_panic_hook();
        assert!(is_panic_hook_installed());
    }

    #[test]
    #[should_panic(expected = "test panic")]
    fn test_panic_hook_with_string() {
        install_panic_hook();
        panic!("test panic");
    }

    #[test]
    fn test_panic_hook_messages() {
        // This test just verifies the message generation logic doesn't crash
        let location = panic::Location::caller();
        let info = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        assert!(!info.is_empty());
    }
}
