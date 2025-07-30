use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use memchr::memmem;

/// Message structure passed from RustMQ to the ETL module
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Message {
    pub key: Option<String>,
    pub value: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub timestamp: i64,
}

/// Result structure returned by the ETL module to RustMQ
#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessResult {
    pub transformed_messages: Vec<Message>,
    pub should_continue: bool,
    pub error: Option<String>,
}

/// WASM interface structure to return both pointer and length
/// This prevents memory leaks by providing explicit size information
#[repr(C)]
pub struct WasmResult {
    pub ptr: *mut u8,
    pub len: usize,
}

/// Configuration for the ETL pipeline (passed during initialization)
#[derive(Deserialize, Debug, Clone)]
pub struct EtlConfig {
    pub filter_rules: FilterConfig,
    pub enrichment_config: EnrichmentConfig,
    pub transformation_config: TransformationConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FilterConfig {
    pub spam_keywords: Vec<String>,
    pub max_message_size: usize,
    pub max_age_seconds: i64,
    pub required_headers: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EnrichmentConfig {
    pub enable_geolocation: bool,
    pub enable_content_analysis: bool,
    pub enable_language_detection: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TransformationConfig {
    pub enable_temperature_conversion: bool,
    pub enable_email_normalization: bool,
    pub enable_coordinate_formatting: bool,
}

impl Default for EtlConfig {
    fn default() -> Self {
        Self {
            filter_rules: FilterConfig {
                spam_keywords: vec!["spam".to_string(), "test-delete".to_string()],
                max_message_size: 1024 * 1024, // 1MB
                max_age_seconds: 3600, // 1 hour
                required_headers: vec!["source".to_string()],
            },
            enrichment_config: EnrichmentConfig {
                enable_geolocation: true,
                enable_content_analysis: true,
                enable_language_detection: true,
            },
            transformation_config: TransformationConfig {
                enable_temperature_conversion: true,
                enable_email_normalization: true,
                enable_coordinate_formatting: true,
            },
        }
    }
}

// Global configuration (initialized once)
static mut ETL_CONFIG: Option<EtlConfig> = None;

/// Initialize the ETL module with configuration
/// This should be called once when the module is loaded
#[no_mangle]
pub extern "C" fn init_module(config_ptr: *const u8, config_len: usize) -> bool {
    if config_len == 0 {
        // Use default configuration
        unsafe {
            ETL_CONFIG = Some(EtlConfig::default());
        }
        return true;
    }

    let config_slice = unsafe { std::slice::from_raw_parts(config_ptr, config_len) };
    
    match bincode::deserialize::<EtlConfig>(config_slice) {
        Ok(config) => {
            unsafe {
                ETL_CONFIG = Some(config);
            }
            true
        }
        Err(_) => {
            // Fallback to default on error
            unsafe {
                ETL_CONFIG = Some(EtlConfig::default());
            }
            false
        }
    }
}

/// Main entry point called by RustMQ for each message
/// Returns a pointer to WasmResult containing both data pointer and length
#[no_mangle]
pub extern "C" fn process_message(input_ptr: *const u8, input_len: usize) -> *mut WasmResult {
    let input_slice = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    
    // Parse message using efficient binary format
    let message: Message = match bincode::deserialize(input_slice) {
        Ok(msg) => msg,
        Err(e) => {
            return create_wasm_result(ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some(format!("Failed to parse message: {}", e)),
            });
        }
    };

    // Ensure configuration is initialized
    if unsafe { ETL_CONFIG.is_none() } {
        unsafe {
            ETL_CONFIG = Some(EtlConfig::default());
        }
    }

    match transform_message(message) {
        Ok(result) => create_wasm_result(result),
        Err(e) => create_wasm_result(ProcessResult {
            transformed_messages: vec![],
            should_continue: false,
            error: Some(e),
        }),
    }
}

/// Core message transformation logic - refactored into clear pipeline
fn transform_message(message: Message) -> Result<ProcessResult, String> {
    let default_config = EtlConfig::default();
    let config = unsafe { 
        ETL_CONFIG.as_ref().unwrap_or(&default_config)
    };

    // Step 1: Apply filtering first (early exit if filtered)
    if should_filter_message(&message, &config.filter_rules) {
        return Ok(ProcessResult {
            transformed_messages: vec![],
            should_continue: true,
            error: None,
        });
    }

    // Step 2: Apply transformations
    let mut message = apply_transformations(message, &config.transformation_config)?;

    // Step 3: Apply enrichment
    message = apply_enrichment(message, &config.enrichment_config)?;

    Ok(ProcessResult {
        transformed_messages: vec![message],
        should_continue: true,
        error: None,
    })
}

/// Apply content-specific transformations based on configuration
fn apply_transformations(mut message: Message, config: &TransformationConfig) -> Result<Message, String> {
    // Add processing timestamp
    message.headers.insert(
        "processed_at".to_string(),
        chrono::Utc::now().timestamp().to_string(),
    );

    // Handle different message types
    if let Some(content_type) = message.headers.get("content-type") {
        match content_type.as_str() {
            "application/json" => message = transform_json_message(message, config)?,
            "text/plain" => message = transform_text_message(message)?,
            _ => {
                // Pass through unknown content types
                message.headers.insert("transformation".to_string(), "passthrough".to_string());
            }
        }
    }

    Ok(message)
}

/// Apply enrichment based on configuration
fn apply_enrichment(mut message: Message, config: &EnrichmentConfig) -> Result<Message, String> {
    // Add standard enrichment headers
    message.headers.insert("enriched".to_string(), "true".to_string());
    message.headers.insert("processor".to_string(), "message-processor-v2".to_string());
    message.headers.insert("processing_time".to_string(), 
        chrono::Utc::now().timestamp_millis().to_string());

    // Conditional enrichment based on configuration
    if config.enable_geolocation {
        if let Some(ip) = message.headers.get("client_ip") {
            if let Ok(geo_data) = lookup_geolocation(ip) {
                message.headers.insert("geo_country".to_string(), geo_data.country);
                message.headers.insert("geo_city".to_string(), geo_data.city);
                message.headers.insert("geo_region".to_string(), geo_data.region);
            }
        }
    }

    if config.enable_content_analysis {
        let content_analysis = analyze_content(&message.value);
        message.headers.insert("content_size".to_string(), message.value.len().to_string());
        message.headers.insert("content_type_detected".to_string(), content_analysis.detected_type);
        
        if config.enable_language_detection {
            message.headers.insert("content_language".to_string(), content_analysis.language);
        }
    }

    Ok(message)
}

/// Transform JSON messages
fn transform_json_message(mut message: Message, config: &TransformationConfig) -> Result<Message, String> {
    let mut json_value: serde_json::Value = serde_json::from_slice(&message.value)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    if let serde_json::Value::Object(ref mut obj) = json_value {
        // Add processing metadata
        obj.insert("_processed".to_string(), serde_json::Value::Bool(true));
        obj.insert("_processor_version".to_string(), serde_json::Value::String("1.0.0".to_string()));
        
        // Transform temperature units (configurable)
        if config.enable_temperature_conversion {
            if let Some(temp_c) = obj.get("temperature_celsius").and_then(|v| v.as_f64()) {
                let temp_f = (temp_c * 9.0 / 5.0) + 32.0;
                // Handle potential NaN/Infinity values gracefully
                if let Some(temp_f_number) = serde_json::Number::from_f64(temp_f) {
                    obj.insert("temperature_fahrenheit".to_string(), temp_f_number.into());
                } else {
                    // Log error or skip field if temperature conversion resulted in invalid number
                    obj.insert("temperature_conversion_error".to_string(), 
                        "Invalid temperature value (NaN or Infinity)".into());
                }
            }
        }

        // Normalize email addresses (configurable)
        if config.enable_email_normalization {
            if let Some(email) = obj.get("email").and_then(|v| v.as_str()) {
                let normalized_email = email.to_lowercase();
                obj.insert("email".to_string(), normalized_email.into());
            }
        }

        // Add computed fields (configurable)
        if config.enable_coordinate_formatting {
            if let (Some(lat), Some(lon)) = (
                obj.get("latitude").and_then(|v| v.as_f64()),
                obj.get("longitude").and_then(|v| v.as_f64())
            ) {
                obj.insert("coordinate_string".to_string(), 
                    format!("{:.6},{:.6}", lat, lon).into());
            }
        }
    }

    message.value = serde_json::to_vec(&json_value)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    Ok(message)
}

/// Transform plain text messages
fn transform_text_message(mut message: Message) -> Result<Message, String> {
    let text = std::str::from_utf8(&message.value)
        .map_err(|e| format!("Invalid UTF-8: {}", e))?;
    
    let original_length = text.len();
    
    // Convert to uppercase and add metadata
    let transformed_text = format!("[PROCESSED] {}", text.to_uppercase());
    message.value = transformed_text.into_bytes();
    
    message.headers.insert("transformation".to_string(), "text_uppercase".to_string());
    message.headers.insert("original_length".to_string(), original_length.to_string());

    Ok(message)
}

/// Determine if a message should be filtered out using efficient byte search
fn should_filter_message(message: &Message, config: &FilterConfig) -> bool {
    // Filter messages without required headers
    for required_header in &config.required_headers {
        if !message.headers.contains_key(required_header) {
            return true;
        }
    }

    // Filter oversized messages
    if message.value.len() > config.max_message_size {
        return true;
    }

    // Filter based on spam keywords using efficient byte search
    for spam_keyword in &config.spam_keywords {
        if memmem::find(&message.value, spam_keyword.as_bytes()).is_some() {
            return true;
        }
        // Also check case-insensitive by converting to lowercase bytes
        let keyword_lower = spam_keyword.to_lowercase();
        if let Ok(text) = std::str::from_utf8(&message.value) {
            let text_lower = text.to_lowercase();
            if memmem::find(text_lower.as_bytes(), keyword_lower.as_bytes()).is_some() {
                return true;
            }
        }
    }

    // Filter old messages
    let current_time = chrono::Utc::now().timestamp();
    if message.timestamp < current_time - config.max_age_seconds {
        return true;
    }

    false
}


/// Simple geolocation lookup based on IP
struct GeoData {
    country: String,
    city: String,
    region: String,
}

fn lookup_geolocation(ip: &str) -> Result<GeoData, String> {
    // Simple IP-based geolocation (replace with actual service in production)
    if ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("172.") {
        Ok(GeoData {
            country: "Local".to_string(),
            city: "Private Network".to_string(),
            region: "RFC1918".to_string(),
        })
    } else if ip.starts_with("8.8.") || ip.starts_with("8.4.") {
        Ok(GeoData {
            country: "US".to_string(),
            city: "Mountain View".to_string(),
            region: "California".to_string(),
        })
    } else if ip.starts_with("1.1.") {
        Ok(GeoData {
            country: "US".to_string(),
            city: "San Francisco".to_string(),
            region: "California".to_string(),
        })
    } else {
        Ok(GeoData {
            country: "Unknown".to_string(),
            city: "Unknown".to_string(),
            region: "Unknown".to_string(),
        })
    }
}

/// Content analysis structure
struct ContentAnalysis {
    detected_type: String,
    language: String,
}

/// Analyze message content to detect type and language
fn analyze_content(content: &[u8]) -> ContentAnalysis {
    if let Ok(text) = std::str::from_utf8(content) {
        // Try to detect JSON
        if text.trim_start().starts_with('{') && text.trim_end().ends_with('}') {
            if serde_json::from_str::<serde_json::Value>(text).is_ok() {
                return ContentAnalysis {
                    detected_type: "application/json".to_string(),
                    language: detect_language(text),
                };
            }
        }

        // Try to detect XML
        if text.trim_start().starts_with('<') {
            return ContentAnalysis {
                detected_type: "application/xml".to_string(),
                language: detect_language(text),
            };
        }

        // Default to plain text
        ContentAnalysis {
            detected_type: "text/plain".to_string(),
            language: detect_language(text),
        }
    } else {
        ContentAnalysis {
            detected_type: "application/octet-stream".to_string(),
            language: "unknown".to_string(),
        }
    }
}

/// Simple language detection based on common words
fn detect_language(text: &str) -> String {
    let text_lower = text.to_lowercase();
    
    // English indicators
    if text_lower.contains(" the ") || text_lower.contains(" and ") || text_lower.contains(" is ") {
        return "en".to_string();
    }
    
    // Spanish indicators
    if text_lower.contains(" el ") || text_lower.contains(" la ") || text_lower.contains(" es ") {
        return "es".to_string();
    }
    
    // French indicators
    if text_lower.contains(" le ") || text_lower.contains(" la ") || text_lower.contains(" est ") {
        return "fr".to_string();
    }
    
    "unknown".to_string()
}

/// Create WASM result with proper memory management
fn create_wasm_result(result: ProcessResult) -> *mut WasmResult {
    // Serialize using efficient binary format
    let serialized = match bincode::serialize(&result) {
        Ok(data) => data,
        Err(_) => {
            let error_result = ProcessResult {
                transformed_messages: vec![],
                should_continue: false,
                error: Some("Serialization failed".to_string()),
            };
            bincode::serialize(&error_result).unwrap_or_else(|_| vec![0])
        }
    };

    // Allocate buffer for the data
    let len = serialized.len();
    let ptr = alloc(len);
    
    // Copy data to allocated buffer
    unsafe {
        std::ptr::copy_nonoverlapping(serialized.as_ptr(), ptr, len);
    }

    // Create WasmResult structure
    let wasm_result = WasmResult { ptr, len };
    
    // Allocate and return pointer to WasmResult
    let result_ptr = alloc(std::mem::size_of::<WasmResult>()) as *mut WasmResult;
    unsafe {
        std::ptr::write(result_ptr, wasm_result);
    }
    
    result_ptr
}

/// Memory allocation for WASM
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Memory deallocation for WASM
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_transformation() {
        let message = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": 25.0, "sensor_id": "temp001", "email": "TEST@EXAMPLE.COM"}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = transform_message(message).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        let transformed = &result.transformed_messages[0];
        let json: serde_json::Value = serde_json::from_slice(&transformed.value).unwrap();
        
        assert!(json.get("_processed").unwrap().as_bool().unwrap());
        assert_eq!(json.get("temperature_fahrenheit").unwrap().as_f64().unwrap(), 77.0);
        assert_eq!(json.get("email").unwrap().as_str().unwrap(), "test@example.com");
    }

    #[test]
    fn test_text_transformation() {
        let message = Message {
            key: Some("text-key".to_string()),
            value: b"hello world".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "text/plain".to_string());
                h.insert("source".to_string(), "user-input".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = transform_message(message).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        let transformed = &result.transformed_messages[0];
        let text = std::str::from_utf8(&transformed.value).unwrap();
        assert!(text.contains("[PROCESSED] HELLO WORLD"));
        assert_eq!(transformed.headers.get("transformation").unwrap(), "text_uppercase");
    }

    #[test]
    fn test_message_filtering() {
        let filter_config = FilterConfig {
            spam_keywords: vec!["spam".to_string()],
            max_message_size: 1024 * 1024,
            max_age_seconds: 3600,
            required_headers: vec!["source".to_string()],
        };
        
        // Test spam filtering
        let spam_message = Message {
            key: Some("spam-key".to_string()),
            value: b"This is spam content".to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("source".to_string(), "unknown".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        assert!(should_filter_message(&spam_message, &filter_config));

        // Test missing source header
        let no_source_message = Message {
            key: Some("test-key".to_string()),
            value: b"valid content".to_vec(),
            headers: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        assert!(should_filter_message(&no_source_message, &filter_config));
    }

    #[test]
    fn test_geolocation_lookup() {
        let private_ip_geo = lookup_geolocation("192.168.1.1").unwrap();
        assert_eq!(private_ip_geo.country, "Local");

        let google_dns_geo = lookup_geolocation("8.8.8.8").unwrap();
        assert_eq!(google_dns_geo.country, "US");
        assert_eq!(google_dns_geo.city, "Mountain View");
    }

    #[test]
    fn test_content_analysis() {
        let json_analysis = analyze_content(br#"{"test": true}"#);
        assert_eq!(json_analysis.detected_type, "application/json");

        let xml_analysis = analyze_content(b"<root><test>value</test></root>");
        assert_eq!(xml_analysis.detected_type, "application/xml");

        let text_analysis = analyze_content(b"Hello world, this is a test");
        assert_eq!(text_analysis.detected_type, "text/plain");
        assert_eq!(text_analysis.language, "en");
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(detect_language("Hello, this is the test"), "en");
        assert_eq!(detect_language("Hola, este es el test"), "es");
        assert_eq!(detect_language("Bonjour, c'est le test"), "fr");
        assert_eq!(detect_language("Something else"), "unknown");
    }

    #[test]
    fn test_temperature_conversion_with_invalid_values() {
        // Test with NaN temperature value
        let message_nan = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": "invalid"}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Should handle invalid temperature gracefully
        let result = transform_message(message_nan).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);

        // Test with a JSON that would produce NaN when processed
        let message_with_nan_calc = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": null}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Should handle null temperature value gracefully (gets skipped)
        let result = transform_message(message_with_nan_calc).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);

        // Test with extremely large temperature that could result in infinity
        // Use a value that will cause overflow during calculation: large enough that (x * 9/5 + 32) = infinity
        let message_large_temp = Message {
            key: Some("test-key".to_string()),
            value: r#"{"temperature_celsius": 1e308}"#.as_bytes().to_vec(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("source".to_string(), "sensor-network".to_string());
                h
            },
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Should handle extreme temperature values gracefully
        let result = transform_message(message_large_temp).unwrap();
        assert_eq!(result.transformed_messages.len(), 1);
        
        // Check that error was recorded instead of causing panic
        let transformed = &result.transformed_messages[0];
        let json: serde_json::Value = serde_json::from_slice(&transformed.value).unwrap();
        
        // Should have either temperature_fahrenheit or temperature_conversion_error
        let has_temp_f = json.get("temperature_fahrenheit").is_some();
        let has_temp_error = json.get("temperature_conversion_error").is_some();
        assert!(has_temp_f || has_temp_error, "Should have either converted temperature or error message");
    }
}