use crate::config::{TopicFilter, FilterType, ConditionalRule, ConditionType, ComparisonOperator};
use crate::{Result, error::RustMqError, types::Record};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use regex::Regex;
use glob::Pattern as GlobPattern;
use serde_json::Value as JsonValue;
use std::time::{SystemTime, UNIX_EPOCH};
use bytes::Bytes;

/// High-performance topic filter engine with optimized matching strategies
pub struct TopicFilterEngine {
    /// O(1) exact string matches using hash set
    exact_matches: HashSet<String>,
    /// Case-insensitive exact matches (lowercase)
    exact_matches_insensitive: HashSet<String>,
    /// Compiled glob patterns for wildcard matching
    wildcard_patterns: Vec<CompiledGlobFilter>,
    /// Compiled regex patterns with caching
    regex_patterns: Vec<CompiledRegexFilter>,
    /// Prefix patterns for efficient prefix matching
    prefix_patterns: Vec<StringFilter>,
    /// Suffix patterns for efficient suffix matching
    suffix_patterns: Vec<StringFilter>,
    /// Contains patterns for substring matching
    contains_patterns: Vec<StringFilter>,
}

/// Compiled glob filter with metadata
#[derive(Clone)]
pub struct CompiledGlobFilter {
    pub pattern: GlobPattern,
    pub original: String,
    pub case_sensitive: bool,
    pub negate: bool,
}

/// Compiled regex filter with metadata
#[derive(Clone)]
pub struct CompiledRegexFilter {
    pub regex: Regex,
    pub original: String,
    pub case_sensitive: bool,
    pub negate: bool,
}

/// String-based filter (prefix, suffix, contains)
#[derive(Clone)]
pub struct StringFilter {
    pub pattern: String,
    pub case_sensitive: bool,
    pub negate: bool,
}

/// Conditional rule evaluation engine
pub struct ConditionalRuleEngine {
    /// Compiled rules for efficient evaluation
    rules: Vec<CompiledConditionalRule>,
}

/// Compiled conditional rule with optimized evaluation
pub struct CompiledConditionalRule {
    pub condition_type: ConditionType,
    pub field_path: String,
    pub operator: ComparisonOperator,
    pub value: JsonValue,
    pub negate: bool,
    /// Pre-compiled regex for pattern matching
    pub compiled_regex: Option<Regex>,
}

/// Filter evaluation result
#[derive(Debug, Clone)]
pub struct FilterResult {
    pub matches: bool,
    pub matched_patterns: Vec<String>,
    pub evaluation_time_ns: u64,
}

impl TopicFilterEngine {
    /// Create a new optimized topic filter engine
    pub fn new(filters: Vec<TopicFilter>) -> Result<Self> {
        let mut exact_matches = HashSet::new();
        let mut exact_matches_insensitive = HashSet::new();
        let mut wildcard_patterns = Vec::new();
        let mut regex_patterns = Vec::new();
        let mut prefix_patterns = Vec::new();
        let mut suffix_patterns = Vec::new();
        let mut contains_patterns = Vec::new();

        for filter in filters {
            match filter.filter_type {
                FilterType::Exact => {
                    if filter.case_sensitive {
                        exact_matches.insert(filter.pattern);
                    } else {
                        exact_matches_insensitive.insert(filter.pattern.to_lowercase());
                    }
                }
                FilterType::Wildcard => {
                    let glob_pattern = GlobPattern::new(&filter.pattern)
                        .map_err(|e| RustMqError::ConfigurationError(
                            format!("Invalid glob pattern '{}': {}", filter.pattern, e)
                        ))?;
                    wildcard_patterns.push(CompiledGlobFilter {
                        pattern: glob_pattern,
                        original: filter.pattern,
                        case_sensitive: filter.case_sensitive,
                        negate: filter.negate,
                    });
                }
                FilterType::Regex => {
                    let regex = regex::RegexBuilder::new(&filter.pattern)
                        .case_insensitive(!filter.case_sensitive)
                        .build()
                        .map_err(|e| RustMqError::ConfigurationError(
                            format!("Invalid regex pattern '{}': {}", filter.pattern, e)
                        ))?;
                    regex_patterns.push(CompiledRegexFilter {
                        regex,
                        original: filter.pattern,
                        case_sensitive: filter.case_sensitive,
                        negate: filter.negate,
                    });
                }
                FilterType::Prefix => {
                    prefix_patterns.push(StringFilter {
                        pattern: filter.pattern,
                        case_sensitive: filter.case_sensitive,
                        negate: filter.negate,
                    });
                }
                FilterType::Suffix => {
                    suffix_patterns.push(StringFilter {
                        pattern: filter.pattern,
                        case_sensitive: filter.case_sensitive,
                        negate: filter.negate,
                    });
                }
                FilterType::Contains => {
                    contains_patterns.push(StringFilter {
                        pattern: filter.pattern,
                        case_sensitive: filter.case_sensitive,
                        negate: filter.negate,
                    });
                }
            }
        }

        Ok(Self {
            exact_matches,
            exact_matches_insensitive,
            wildcard_patterns,
            regex_patterns,
            prefix_patterns,
            suffix_patterns,
            contains_patterns,
        })
    }

    /// Evaluate topic against all configured filters
    /// Returns true if ANY filter matches (OR logic)
    pub fn matches_topic(&self, topic: &str) -> FilterResult {
        let start_time = std::time::Instant::now();
        let mut matched_patterns = Vec::new();

        // Fast path: Check exact matches first (O(1))
        if self.exact_matches.contains(topic) {
            matched_patterns.push(format!("exact:{}", topic));
            return FilterResult {
                matches: true,
                matched_patterns,
                evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
            };
        }

        // Check case-insensitive exact matches
        if self.exact_matches_insensitive.contains(&topic.to_lowercase()) {
            matched_patterns.push(format!("exact_insensitive:{}", topic));
            return FilterResult {
                matches: true,
                matched_patterns,
                evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
            };
        }

        // Check prefix patterns (fast linear scan)
        for prefix_filter in &self.prefix_patterns {
            let matches = if prefix_filter.case_sensitive {
                topic.starts_with(&prefix_filter.pattern)
            } else {
                topic.to_lowercase().starts_with(&prefix_filter.pattern.to_lowercase())
            };

            let result = if prefix_filter.negate { !matches } else { matches };
            if result {
                matched_patterns.push(format!("prefix:{}", prefix_filter.pattern));
                return FilterResult {
                    matches: true,
                    matched_patterns,
                    evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
                };
            }
        }

        // Check suffix patterns
        for suffix_filter in &self.suffix_patterns {
            let matches = if suffix_filter.case_sensitive {
                topic.ends_with(&suffix_filter.pattern)
            } else {
                topic.to_lowercase().ends_with(&suffix_filter.pattern.to_lowercase())
            };

            let result = if suffix_filter.negate { !matches } else { matches };
            if result {
                matched_patterns.push(format!("suffix:{}", suffix_filter.pattern));
                return FilterResult {
                    matches: true,
                    matched_patterns,
                    evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
                };
            }
        }

        // Check contains patterns
        for contains_filter in &self.contains_patterns {
            let matches = if contains_filter.case_sensitive {
                topic.contains(&contains_filter.pattern)
            } else {
                topic.to_lowercase().contains(&contains_filter.pattern.to_lowercase())
            };

            let result = if contains_filter.negate { !matches } else { matches };
            if result {
                matched_patterns.push(format!("contains:{}", contains_filter.pattern));
                return FilterResult {
                    matches: true,
                    matched_patterns,
                    evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
                };
            }
        }

        // Check wildcard patterns (compiled globs)
        for wildcard_filter in &self.wildcard_patterns {
            let test_topic = if wildcard_filter.case_sensitive {
                topic
            } else {
                &topic.to_lowercase()
            };

            let matches = wildcard_filter.pattern.matches(test_topic);
            let result = if wildcard_filter.negate { !matches } else { matches };
            if result {
                matched_patterns.push(format!("wildcard:{}", wildcard_filter.original));
                return FilterResult {
                    matches: true,
                    matched_patterns,
                    evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
                };
            }
        }

        // Check regex patterns (most expensive, done last)
        for regex_filter in &self.regex_patterns {
            let matches = regex_filter.regex.is_match(topic);
            let result = if regex_filter.negate { !matches } else { matches };
            if result {
                matched_patterns.push(format!("regex:{}", regex_filter.original));
                return FilterResult {
                    matches: true,
                    matched_patterns,
                    evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
                };
            }
        }

        // No matches found
        FilterResult {
            matches: false,
            matched_patterns,
            evaluation_time_ns: start_time.elapsed().as_nanos() as u64,
        }
    }

    /// Get performance statistics for this filter engine
    pub fn get_stats(&self) -> FilterEngineStats {
        FilterEngineStats {
            exact_patterns: self.exact_matches.len() + self.exact_matches_insensitive.len(),
            wildcard_patterns: self.wildcard_patterns.len(),
            regex_patterns: self.regex_patterns.len(),
            prefix_patterns: self.prefix_patterns.len(),
            suffix_patterns: self.suffix_patterns.len(),
            contains_patterns: self.contains_patterns.len(),
        }
    }
}

impl ConditionalRuleEngine {
    /// Create a new conditional rule engine
    pub fn new(rules: Vec<ConditionalRule>) -> Result<Self> {
        let mut compiled_rules = Vec::new();

        for rule in rules {
            let compiled_regex = if rule.operator == ComparisonOperator::Matches {
                match &rule.value {
                    JsonValue::String(pattern) => {
                        Some(Regex::new(pattern)
                            .map_err(|e| RustMqError::ConfigurationError(
                                format!("Invalid regex pattern in conditional rule: {}", e)
                            ))?)
                    }
                    _ => {
                        return Err(RustMqError::ConfigurationError(
                            "Regex operator requires string value".to_string()
                        ));
                    }
                }
            } else {
                None
            };

            compiled_rules.push(CompiledConditionalRule {
                condition_type: rule.condition_type,
                field_path: rule.field_path,
                operator: rule.operator,
                value: rule.value,
                negate: rule.negate,
                compiled_regex,
            });
        }

        Ok(Self {
            rules: compiled_rules,
        })
    }

    /// Evaluate all conditional rules against a record
    /// Returns true if ALL rules match (AND logic)
    pub fn evaluate_record(&self, record: &Record, topic: &str) -> Result<bool> {
        for rule in &self.rules {
            let matches = self.evaluate_single_rule(rule, record, topic)?;
            let result = if rule.negate { !matches } else { matches };
            
            if !result {
                return Ok(false); // AND logic: all must match
            }
        }
        Ok(true)
    }

    /// Evaluate a single conditional rule
    fn evaluate_single_rule(&self, rule: &CompiledConditionalRule, record: &Record, topic: &str) -> Result<bool> {
        let extracted_value = match rule.condition_type {
            ConditionType::HeaderValue => {
                self.extract_header_value(record, &rule.field_path)
            }
            ConditionType::PayloadField => {
                self.extract_payload_field(record, &rule.field_path)?
            }
            ConditionType::MessageSize => {
                JsonValue::Number(serde_json::Number::from(record.value.len()))
            }
            ConditionType::MessageAge => {
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                let age_ms = current_time - record.timestamp;
                JsonValue::Number(serde_json::Number::from(age_ms))
            }
        };

        self.compare_values(&extracted_value, &rule.value, &rule.operator, rule.compiled_regex.as_ref())
    }

    /// Extract header value by key
    fn extract_header_value(&self, record: &Record, field_path: &str) -> JsonValue {
        for header in &record.headers {
            if header.key == field_path {
                if let Ok(string_value) = String::from_utf8(header.value.to_vec()) { // Convert Bytes to Vec<u8>
                    return JsonValue::String(string_value);
                }
            }
        }
        JsonValue::Null
    }

    /// Extract field from JSON payload using simplified JSONPath
    fn extract_payload_field(&self, record: &Record, field_path: &str) -> Result<JsonValue> {
        // Parse the record payload as JSON
        let payload: JsonValue = serde_json::from_slice(&record.value)
            .map_err(|e| RustMqError::EtlProcessingFailed(
                format!("Failed to parse payload as JSON: {}", e)
            ))?;

        // Simplified JSONPath implementation for common cases
        if field_path.starts_with("$.") {
            let path = &field_path[2..]; // Remove "$."
            self.extract_json_field(&payload, path)
        } else {
            self.extract_json_field(&payload, field_path)
        }
    }

    /// Extract field from JSON using dot notation
    fn extract_json_field(&self, value: &JsonValue, path: &str) -> Result<JsonValue> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                JsonValue::Object(map) => {
                    current = map.get(part).unwrap_or(&JsonValue::Null);
                }
                JsonValue::Array(arr) => {
                    if let Ok(index) = part.parse::<usize>() {
                        current = arr.get(index).unwrap_or(&JsonValue::Null);
                    } else {
                        return Ok(JsonValue::Null);
                    }
                }
                _ => return Ok(JsonValue::Null),
            }
        }

        Ok(current.clone())
    }

    /// Compare two JSON values using the specified operator
    fn compare_values(&self, left: &JsonValue, right: &JsonValue, operator: &ComparisonOperator, regex: Option<&Regex>) -> Result<bool> {
        match operator {
            ComparisonOperator::Equals => Ok(left == right),
            ComparisonOperator::NotEquals => Ok(left != right),
            ComparisonOperator::GreaterThan => self.compare_numeric(left, right, |a, b| a > b),
            ComparisonOperator::LessThan => self.compare_numeric(left, right, |a, b| a < b),
            ComparisonOperator::GreaterThanOrEqual => self.compare_numeric(left, right, |a, b| a >= b),
            ComparisonOperator::LessThanOrEqual => self.compare_numeric(left, right, |a, b| a <= b),
            ComparisonOperator::Contains => self.compare_string_contains(left, right),
            ComparisonOperator::StartsWith => self.compare_string_starts_with(left, right),
            ComparisonOperator::EndsWith => self.compare_string_ends_with(left, right),
            ComparisonOperator::Matches => {
                if let Some(regex) = regex {
                    if let JsonValue::String(s) = left {
                        Ok(regex.is_match(s))
                    } else {
                        Ok(false)
                    }
                } else {
                    Err(RustMqError::EtlProcessingFailed(
                        "Regex not compiled for Matches operator".to_string()
                    ))
                }
            }
        }
    }

    /// Compare numeric values
    fn compare_numeric<F>(&self, left: &JsonValue, right: &JsonValue, comparator: F) -> Result<bool>
    where
        F: Fn(f64, f64) -> bool,
    {
        let left_num = match left {
            JsonValue::Number(n) => n.as_f64().unwrap_or(0.0),
            _ => return Ok(false),
        };

        let right_num = match right {
            JsonValue::Number(n) => n.as_f64().unwrap_or(0.0),
            _ => return Ok(false),
        };

        Ok(comparator(left_num, right_num))
    }

    /// Compare string contains
    fn compare_string_contains(&self, left: &JsonValue, right: &JsonValue) -> Result<bool> {
        match (left, right) {
            (JsonValue::String(left_str), JsonValue::String(right_str)) => {
                Ok(left_str.contains(right_str))
            }
            _ => Ok(false),
        }
    }

    /// Compare string starts with
    fn compare_string_starts_with(&self, left: &JsonValue, right: &JsonValue) -> Result<bool> {
        match (left, right) {
            (JsonValue::String(left_str), JsonValue::String(right_str)) => {
                Ok(left_str.starts_with(right_str))
            }
            _ => Ok(false),
        }
    }

    /// Compare string ends with
    fn compare_string_ends_with(&self, left: &JsonValue, right: &JsonValue) -> Result<bool> {
        match (left, right) {
            (JsonValue::String(left_str), JsonValue::String(right_str)) => {
                Ok(left_str.ends_with(right_str))
            }
            _ => Ok(false),
        }
    }
}

/// Performance statistics for filter engine
#[derive(Debug, Clone)]
pub struct FilterEngineStats {
    pub exact_patterns: usize,
    pub wildcard_patterns: usize,
    pub regex_patterns: usize,
    pub prefix_patterns: usize,
    pub suffix_patterns: usize,
    pub contains_patterns: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Header;

    #[test]
    fn test_exact_topic_filtering() {
        let filters = vec![
            TopicFilter {
                filter_type: FilterType::Exact,
                pattern: "events.user.signup".to_string(),
                case_sensitive: true,
                negate: false,
            },
            TopicFilter {
                filter_type: FilterType::Exact,
                pattern: "logs.error".to_string(),
                case_sensitive: false,
                negate: false,
            },
        ];

        let engine = TopicFilterEngine::new(filters).unwrap();

        // Test exact match
        let result = engine.matches_topic("events.user.signup");
        assert!(result.matches);
        assert_eq!(result.matched_patterns[0], "exact:events.user.signup");

        // Test case insensitive match
        let result = engine.matches_topic("LOGS.ERROR");
        assert!(result.matches);

        // Test no match
        let result = engine.matches_topic("events.user.login");
        assert!(!result.matches);
    }

    #[test]
    fn test_wildcard_topic_filtering() {
        let filters = vec![
            TopicFilter {
                filter_type: FilterType::Wildcard,
                pattern: "events.*".to_string(),
                case_sensitive: true,
                negate: false,
            },
            TopicFilter {
                filter_type: FilterType::Wildcard,
                pattern: "*.error".to_string(),
                case_sensitive: true,
                negate: false,
            },
        ];

        let engine = TopicFilterEngine::new(filters).unwrap();

        assert!(engine.matches_topic("events.user.signup").matches);
        assert!(engine.matches_topic("events.order.created").matches);
        assert!(engine.matches_topic("logs.error").matches);
        assert!(!engine.matches_topic("metrics.cpu.usage").matches);
    }

    #[test]
    fn test_regex_topic_filtering() {
        let filters = vec![
            TopicFilter {
                filter_type: FilterType::Regex,
                pattern: r"^sensor-\d+$".to_string(),
                case_sensitive: true,
                negate: false,
            },
        ];

        let engine = TopicFilterEngine::new(filters).unwrap();

        assert!(engine.matches_topic("sensor-123").matches);
        assert!(engine.matches_topic("sensor-999").matches);
        assert!(!engine.matches_topic("sensor-abc").matches);
        assert!(!engine.matches_topic("device-123").matches);
    }

    #[test]
    fn test_prefix_suffix_filtering() {
        let filters = vec![
            TopicFilter {
                filter_type: FilterType::Prefix,
                pattern: "logs.".to_string(),
                case_sensitive: true,
                negate: false,
            },
            TopicFilter {
                filter_type: FilterType::Suffix,
                pattern: ".critical".to_string(),
                case_sensitive: true,
                negate: false,
            },
        ];

        let engine = TopicFilterEngine::new(filters).unwrap();

        assert!(engine.matches_topic("logs.application").matches);
        assert!(engine.matches_topic("logs.system.error").matches);
        assert!(engine.matches_topic("events.alert.critical").matches);
        assert!(!engine.matches_topic("events.user.signup").matches);
    }

    #[test]
    fn test_conditional_rule_header_evaluation() {
        let rules = vec![
            ConditionalRule {
                condition_type: ConditionType::HeaderValue,
                field_path: "content-type".to_string(),
                operator: ComparisonOperator::Equals,
                value: JsonValue::String("application/json".to_string()),
                negate: false,
            },
        ];

        let engine = ConditionalRuleEngine::new(rules).unwrap();

        let record = Record::new(
            Some(b"test".to_vec()),
            b"{}".to_vec(),
            vec![
                Header::new(
                    "content-type".to_string(),
                    b"application/json".to_vec(),
                ),
            ],
            1234567890,
        );

        assert!(engine.evaluate_record(&record, "test.topic").unwrap());
    }

    #[test]
    fn test_conditional_rule_payload_evaluation() {
        let rules = vec![
            ConditionalRule {
                condition_type: ConditionType::PayloadField,
                field_path: "user.id".to_string(),
                operator: ComparisonOperator::Equals,
                value: JsonValue::String("user123".to_string()),
                negate: false,
            },
        ];

        let engine = ConditionalRuleEngine::new(rules).unwrap();

        let payload = r#"{"user": {"id": "user123", "name": "John"}}"#;
        let record = Record::new(
            Some(b"test".to_vec()),
            payload.as_bytes().to_vec(),
            vec![],
            1234567890,
        );

        assert!(engine.evaluate_record(&record, "test.topic").unwrap());
    }

    #[test]
    fn test_conditional_rule_message_size() {
        let rules = vec![
            ConditionalRule {
                condition_type: ConditionType::MessageSize,
                field_path: "".to_string(),
                operator: ComparisonOperator::GreaterThan,
                value: JsonValue::Number(serde_json::Number::from(10)),
                negate: false,
            },
        ];

        let engine = ConditionalRuleEngine::new(rules).unwrap();

        let record = Record::new(
            Some(b"test".to_vec()),
            b"this is a long message".to_vec(),
            vec![],
            1234567890,
        );

        assert!(engine.evaluate_record(&record, "test.topic").unwrap());
    }

    #[test]
    fn test_filter_performance() {
        // Create a large number of filters to test performance
        let mut filters = Vec::new();
        
        // Add 100 exact matches
        for i in 0..100 {
            filters.push(TopicFilter {
                filter_type: FilterType::Exact,
                pattern: format!("topic-{}", i),
                case_sensitive: true,
                negate: false,
            });
        }

        let engine = TopicFilterEngine::new(filters).unwrap();
        
        // Test performance of exact match (should be very fast)
        let result = engine.matches_topic("topic-50");
        assert!(result.matches);
        assert!(result.evaluation_time_ns < 1_000_000); // Should be under 1ms
    }
}