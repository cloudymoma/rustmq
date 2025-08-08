use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};
use std::str::FromStr;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
    Csv,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            "yaml" => Ok(OutputFormat::Yaml),
            "csv" => Ok(OutputFormat::Csv),
            _ => Err(format!("Invalid output format: {}. Valid formats: table, json, yaml, csv", s)),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Yaml => write!(f, "yaml"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
}

/// Format output based on the specified format
pub fn format_output<T>(data: &T, format: OutputFormat, no_color: bool) -> Result<String, Box<dyn std::error::Error>>
where
    T: Serialize,
{
    match format {
        OutputFormat::Json => format_json(data),
        OutputFormat::Yaml => format_yaml(data),
        OutputFormat::Csv => format_csv(data),
        OutputFormat::Table => format_table(data, no_color),
    }
}

fn format_json<T: Serialize>(data: &T) -> Result<String, Box<dyn std::error::Error>> {
    Ok(serde_json::to_string_pretty(data)?)
}

fn format_yaml<T: Serialize>(data: &T) -> Result<String, Box<dyn std::error::Error>> {
    // Simple YAML-like output since we don't have serde_yaml dependency
    let json_value: Value = serde_json::to_value(data)?;
    Ok(format_yaml_value(&json_value, 0))
}

fn format_yaml_value(value: &Value, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", s),
        Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let items: Vec<String> = arr
                    .iter()
                    .map(|item| format!("{}  - {}", indent_str, format_yaml_value(item, indent + 1)))
                    .collect();
                format!("\n{}", items.join("\n"))
            }
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                "{}".to_string()
            } else {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(key, val)| {
                        if matches!(val, Value::Object(_) | Value::Array(_)) {
                            format!("{}{}: {}", indent_str, key, format_yaml_value(val, indent + 1))
                        } else {
                            format!("{}{}: {}", indent_str, key, format_yaml_value(val, indent))
                        }
                    })
                    .collect();
                if indent == 0 {
                    items.join("\n")
                } else {
                    format!("\n{}", items.join("\n"))
                }
            }
        }
    }
}

fn format_csv<T: Serialize>(data: &T) -> Result<String, Box<dyn std::error::Error>> {
    let json_value: Value = serde_json::to_value(data)?;
    
    match json_value {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(String::new());
            }
            
            // Extract headers from the first object
            if let Some(Value::Object(first_obj)) = arr.first() {
                let headers: Vec<String> = first_obj.keys().cloned().collect();
                let mut csv_lines = vec![headers.join(",")];
                
                for item in arr {
                    if let Value::Object(obj) = item {
                        let values: Vec<String> = headers
                            .iter()
                            .map(|header| {
                                obj.get(header)
                                    .map(|v| csv_escape_value(v))
                                    .unwrap_or_else(|| "".to_string())
                            })
                            .collect();
                        csv_lines.push(values.join(","));
                    }
                }
                
                Ok(csv_lines.join("\n"))
            } else {
                Ok(String::new())
            }
        }
        Value::Object(obj) => {
            // Single object - convert to two-column CSV
            let mut csv_lines = vec!["field,value".to_string()];
            for (key, value) in obj {
                csv_lines.push(format!("{},{}", key, csv_escape_value(&value)));
            }
            Ok(csv_lines.join("\n"))
        }
        _ => {
            // Scalar value
            Ok(csv_escape_value(&json_value))
        }
    }
}

pub fn csv_escape_value(value: &Value) -> String {
    let str_value = match value {
        Value::Null => "".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    };
    
    if str_value.contains(',') || str_value.contains('"') || str_value.contains('\n') {
        format!("\"{}\"", str_value.replace('"', "\"\""))
    } else {
        str_value
    }
}

fn format_table<T: Serialize>(data: &T, no_color: bool) -> Result<String, Box<dyn std::error::Error>> {
    let json_value: Value = serde_json::to_value(data)?;
    
    match json_value {
        Value::Array(arr) => format_table_array(&arr, no_color),
        Value::Object(obj) => format_table_object(&obj, no_color),
        _ => Ok(json_value.to_string()),
    }
}

fn format_table_array(arr: &[Value], no_color: bool) -> Result<String, Box<dyn std::error::Error>> {
    if arr.is_empty() {
        return Ok("No data available".to_string());
    }

    // Check if all elements are objects with the same structure
    if let Some(Value::Object(first_obj)) = arr.first() {
        let headers: Vec<String> = first_obj.keys().cloned().collect();
        
        // Calculate column widths
        let mut col_widths: HashMap<String, usize> = headers
            .iter()
            .map(|h| (h.clone(), h.len()))
            .collect();
            
        for item in arr {
            if let Value::Object(obj) = item {
                for (key, value) in obj {
                    if let Some(width) = col_widths.get_mut(key) {
                        let value_str = format_table_value(value);
                        *width = (*width).max(value_str.len());
                    }
                }
            }
        }
        
        let mut result = Vec::new();
        
        // Header row
        let header_row = headers
            .iter()
            .map(|h| format!("{:<width$}", h.to_uppercase(), width = col_widths[h]))
            .collect::<Vec<_>>()
            .join(" | ");
        result.push(if no_color { header_row } else { format!("\x1b[1m{}\x1b[0m", header_row) });
        
        // Separator
        let separator = headers
            .iter()
            .map(|h| "=".repeat(col_widths[h]))
            .collect::<Vec<_>>()
            .join("=+=");
        result.push(separator);
        
        // Data rows
        for item in arr {
            if let Value::Object(obj) = item {
                let row = headers
                    .iter()
                    .map(|header| {
                        let value_str = obj.get(header)
                            .map(|v| format_table_value(v))
                            .unwrap_or_else(|| "".to_string());
                        format!("{:<width$}", value_str, width = col_widths[header])
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                result.push(row);
            }
        }
        
        Ok(result.join("\n"))
    } else {
        // Array of non-objects, format as simple list
        let items: Vec<String> = arr
            .iter()
            .enumerate()
            .map(|(i, item)| format!("{}: {}", i, format_table_value(item)))
            .collect();
        Ok(items.join("\n"))
    }
}

fn format_table_object(obj: &serde_json::Map<String, Value>, no_color: bool) -> Result<String, Box<dyn std::error::Error>> {
    let key_width = obj.keys().map(|k| k.len()).max().unwrap_or(0);
    
    let mut result = Vec::new();
    
    // Header
    let header = format!("{:<width$} | VALUE", "FIELD", width = key_width);
    result.push(if no_color { header } else { format!("\x1b[1m{}\x1b[0m", header) });
    
    // Separator
    let separator = format!("{}=+={}", "=".repeat(key_width), "=".repeat(20));
    result.push(separator);
    
    // Data rows
    for (key, value) in obj {
        let value_str = format_table_value(value);
        result.push(format!("{:<width$} | {}", key, value_str, width = key_width));
    }
    
    Ok(result.join("\n"))
}

fn format_table_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            // Truncate very long strings for table display
            if s.len() > 50 {
                format!("{}...", &s[..47])
            } else {
                s.clone()
            }
        }
        Value::Array(arr) => {
            if arr.len() <= 3 {
                format!("[{}]", arr.iter().map(|v| format_table_value(v)).collect::<Vec<_>>().join(", "))
            } else {
                format!("[{} items]", arr.len())
            }
        }
        Value::Object(_) => "{object}".to_string(),
    }
}

/// Print formatted output to stdout
pub fn print_output<T>(data: &T, format: OutputFormat, no_color: bool) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize,
{
    let formatted = format_output(data, format, no_color)?;
    println!("{}", formatted);
    Ok(())
}

/// Print error message with appropriate formatting
pub fn print_error(message: &str, no_color: bool) {
    let error_msg = if no_color {
        format!("Error: {}", message)
    } else {
        format!("\x1b[31mError:\x1b[0m {}", message)
    };
    eprintln!("{}", error_msg);
}

/// Print success message with appropriate formatting
pub fn print_success(message: &str, no_color: bool) {
    let success_msg = if no_color {
        format!("Success: {}", message)
    } else {
        format!("\x1b[32mSuccess:\x1b[0m {}", message)
    };
    println!("{}", success_msg);
}

/// Print warning message with appropriate formatting
pub fn print_warning(message: &str, no_color: bool) {
    let warning_msg = if no_color {
        format!("Warning: {}", message)
    } else {
        format!("\x1b[33mWarning:\x1b[0m {}", message)
    };
    println!("{}", warning_msg);
}

/// Print info message with appropriate formatting
pub fn print_info(message: &str, no_color: bool) {
    let info_msg = if no_color {
        format!("Info: {}", message)
    } else {
        format!("\x1b[34mInfo:\x1b[0m {}", message)
    };
    println!("{}", info_msg);
}

/// Confirm destructive operation with user
pub fn confirm_operation(operation: &str, resource: &str) -> bool {
    print!("Are you sure you want to {} '{}'? [y/N]: ", operation, resource);
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let input = input.trim().to_lowercase();
        input == "y" || input == "yes"
    } else {
        false
    }
}

/// Format duration in a human-readable way
pub fn format_duration_days(days: i64) -> String {
    match days {
        d if d < 0 => format!("{} days ago", -d),
        0 => "today".to_string(),
        1 => "1 day".to_string(),
        d if d < 30 => format!("{} days", d),
        d if d < 365 => format!("{} months", d / 30),
        d => format!("{} years", d / 365),
    }
}

/// Format file size in a human-readable way
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format timestamp for display
pub fn format_timestamp(timestamp: &DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Progress indicator for long-running operations
pub struct ProgressIndicator {
    message: String,
    no_color: bool,
}

impl ProgressIndicator {
    pub fn new(message: &str, no_color: bool) -> Self {
        Self {
            message: message.to_string(),
            no_color,
        }
    }
    
    pub fn start(&self) {
        let msg = if self.no_color {
            format!("{} ...", self.message)
        } else {
            format!("\x1b[34m{}\x1b[0m ...", self.message)
        };
        print!("{}", msg);
        io::stdout().flush().unwrap();
    }
    
    pub fn finish(&self, success: bool) {
        let result = if success {
            if self.no_color { " done" } else { "\x1b[32m done\x1b[0m" }
        } else {
            if self.no_color { " failed" } else { "\x1b[31m failed\x1b[0m" }
        };
        println!("{}", result);
    }
}