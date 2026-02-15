// Example showing how to use the rate limiting configuration
// This demonstrates the patterns and how they would be applied

use rustmq::config::{Config, RateLimitConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = Config::default();
    let rate_config = &config.rate_limiting;

    println!("Rate limiting enabled: {}", rate_config.enabled);
    println!(
        "Global RPS limit: {}",
        rate_config.global.requests_per_second
    );
    println!(
        "Per-IP RPS limit: {}",
        rate_config.per_ip.requests_per_second
    );

    // Example of categorizing endpoints
    let test_endpoints = vec![
        ("/health", "Health check"),
        ("/api/v1/cluster", "Cluster status (read)"),
        ("POST:/api/v1/topics", "Create topic (write)"),
        ("/api/v1/cluster/rebalance", "Cluster operation"),
    ];

    for (endpoint, description) in test_endpoints {
        let category = categorize_endpoint(endpoint, &rate_config);
        println!("{}: {} -> {:?}", endpoint, description, category);
    }

    // Show TOML serialization
    println!("\nRate limiting configuration in TOML format:");
    let toml_output = toml::to_string_pretty(rate_config)?;
    println!("{}", toml_output);

    Ok(())
}

// Example function to categorize endpoints based on patterns
fn categorize_endpoint(endpoint: &str, config: &RateLimitConfig) -> Option<&'static str> {
    let endpoints = &config.endpoints;

    // Check health endpoints
    if matches_patterns(endpoint, &endpoints.health.endpoint_patterns) {
        return Some("health");
    }

    // Check write operations (look for HTTP methods)
    if matches_patterns(endpoint, &endpoints.write_operations.endpoint_patterns) {
        return Some("write_operations");
    }

    // Check cluster operations
    if matches_patterns(endpoint, &endpoints.cluster_operations.endpoint_patterns) {
        return Some("cluster_operations");
    }

    // Check read operations (default for API endpoints)
    if matches_patterns(endpoint, &endpoints.read_operations.endpoint_patterns) {
        return Some("read_operations");
    }

    None
}

// Simple pattern matching (in real implementation, you'd use a proper router/matcher)
fn matches_patterns(endpoint: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if pattern.contains("*") {
            // Simple wildcard matching
            let prefix = pattern.trim_end_matches("*");
            endpoint.starts_with(prefix)
        } else if pattern.contains(":") {
            // HTTP method matching
            endpoint == pattern || endpoint.ends_with(&pattern[pattern.find(':').unwrap() + 1..])
        } else {
            // Exact match
            endpoint == pattern
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_categorization() {
        let config = RateLimitConfig::default();

        assert_eq!(categorize_endpoint("/health", &config), Some("health"));
        assert_eq!(
            categorize_endpoint("/api/v1/cluster", &config),
            Some("read_operations")
        );
        assert_eq!(
            categorize_endpoint("POST:/api/v1/topics", &config),
            Some("write_operations")
        );
        assert_eq!(
            categorize_endpoint("/api/v1/cluster/rebalance", &config),
            Some("cluster_operations")
        );
    }

    #[test]
    fn test_pattern_matching() {
        let patterns = vec![
            "/api/v1/topics".to_string(),
            "/api/v1/topics/*".to_string(),
            "POST:/api/v1/topics".to_string(),
        ];

        assert!(matches_patterns("/api/v1/topics", &patterns));
        assert!(matches_patterns("/api/v1/topics/my-topic", &patterns));
        assert!(matches_patterns("POST:/api/v1/topics", &patterns));
        assert!(!matches_patterns("/api/v1/brokers", &patterns));
    }
}
