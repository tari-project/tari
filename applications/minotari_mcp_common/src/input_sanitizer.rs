// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Input sanitization and validation for MCP operations
//!
//! This module provides comprehensive input sanitization following MCP security best practices.
//! It handles HTML entity cleaning, path validation, size limits, and schema-based validation.

use std::collections::HashMap;

use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::{
    config::InputSanitizationConfig,
    error::{McpError, McpResult},
};

/// Input sanitizer for MCP operations
pub struct InputSanitizer {
    max_string_length: usize,
    max_array_length: usize,
    max_object_depth: usize,
    allowed_html_entities: HashMap<String, String>,
}

impl Default for InputSanitizer {
    fn default() -> Self {
        let mut html_entities = HashMap::new();
        html_entities.insert("&amp;".to_string(), "&".to_string());
        html_entities.insert("&lt;".to_string(), "<".to_string());
        html_entities.insert("&gt;".to_string(), ">".to_string());
        html_entities.insert("&quot;".to_string(), "\"".to_string());
        html_entities.insert("&#39;".to_string(), "'".to_string());
        html_entities.insert("&apos;".to_string(), "'".to_string());
        html_entities.insert("&#x27;".to_string(), "'".to_string());
        html_entities.insert("&#x2F;".to_string(), "/".to_string());
        html_entities.insert("&#47;".to_string(), "/".to_string());

        Self {
            max_string_length: 65536, // 64KB max string
            max_array_length: 1000,   // Max 1000 array elements
            max_object_depth: 32,     // Max 32 levels of nesting
            allowed_html_entities: html_entities,
        }
    }
}

impl InputSanitizer {
    /// Create a new input sanitizer with custom limits
    pub fn new(max_string_length: usize, max_array_length: usize, max_object_depth: usize) -> Self {
        Self {
            max_string_length,
            max_array_length,
            max_object_depth,
            ..Default::default()
        }
    }

    /// Create a new input sanitizer from configuration
    pub fn from_config(config: &InputSanitizationConfig) -> Self {
        let mut html_entities = HashMap::new();

        if config.clean_html_entities {
            html_entities.insert("&amp;".to_string(), "&".to_string());
            html_entities.insert("&lt;".to_string(), "<".to_string());
            html_entities.insert("&gt;".to_string(), ">".to_string());
            html_entities.insert("&quot;".to_string(), "\"".to_string());
            html_entities.insert("&#39;".to_string(), "'".to_string());
            html_entities.insert("&apos;".to_string(), "'".to_string());
            html_entities.insert("&#x27;".to_string(), "'".to_string());
            html_entities.insert("&#x2F;".to_string(), "/".to_string());
            html_entities.insert("&#47;".to_string(), "/".to_string());
        }

        Self {
            max_string_length: config.max_string_length,
            max_array_length: config.max_array_length,
            max_object_depth: config.max_object_depth,
            allowed_html_entities: html_entities,
        }
    }

    /// Sanitize and validate JSON input from AI agents
    pub fn sanitize_input(&self, input: &Value) -> McpResult<Value> {
        self.sanitize_value(input, 0)
    }

    /// Recursively sanitize a JSON value
    fn sanitize_value(&self, value: &Value, depth: usize) -> McpResult<Value> {
        // Check depth limit to prevent stack overflow
        if depth > self.max_object_depth {
            return Err(McpError::invalid_request(format!(
                "JSON nesting too deep (max {} levels)",
                self.max_object_depth
            )));
        }

        match value {
            Value::String(s) => self.sanitize_string(s),
            Value::Array(arr) => self.sanitize_array(arr, depth),
            Value::Object(obj) => self.sanitize_object(obj, depth),
            // Numbers, booleans, and null pass through unchanged
            Value::Number(_) | Value::Bool(_) | Value::Null => Ok(value.clone()),
        }
    }

    /// Sanitize a string value
    fn sanitize_string(&self, s: &str) -> McpResult<Value> {
        // Check length limit
        if s.len() > self.max_string_length {
            return Err(McpError::invalid_request(format!(
                "String too long ({} chars, max {})",
                s.len(),
                self.max_string_length
            )));
        }

        // Decode HTML entities
        let mut sanitized = s.to_string();
        for (entity, replacement) in &self.allowed_html_entities {
            sanitized = sanitized.replace(entity, replacement);
        }

        // Remove null bytes and other control characters (except newlines and tabs)
        sanitized = sanitized
            .chars()
            .filter(|&c| c == '\n' || c == '\t' || c == '\r' || (c >= ' ' && c != '\u{007F}') || c >= '\u{0080}')
            .collect();

        // Normalize Unicode (NFC normalization)
        let normalized = sanitized.as_str().nfc().collect::<String>();

        Ok(Value::String(normalized))
    }

    /// Sanitize an array value
    fn sanitize_array(&self, arr: &[Value], depth: usize) -> McpResult<Value> {
        // Check array length limit
        if arr.len() > self.max_array_length {
            return Err(McpError::invalid_request(format!(
                "Array too long ({} elements, max {})",
                arr.len(),
                self.max_array_length
            )));
        }

        let mut sanitized_arr = Vec::with_capacity(arr.len());
        for item in arr {
            let sanitized_item = self.sanitize_value(item, depth + 1)?;
            sanitized_arr.push(sanitized_item);
        }

        Ok(Value::Array(sanitized_arr))
    }

    /// Sanitize an object value
    fn sanitize_object(&self, obj: &serde_json::Map<String, Value>, depth: usize) -> McpResult<Value> {
        // Check object size limit
        if obj.len() > self.max_array_length {
            return Err(McpError::invalid_request(format!(
                "Object too large ({} properties, max {})",
                obj.len(),
                self.max_array_length
            )));
        }

        let mut sanitized_obj = serde_json::Map::new();
        for (key, value) in obj {
            // Sanitize the key (treat as string)
            let sanitized_key = match self.sanitize_string(key)? {
                Value::String(s) => s,
                _ => return Err(McpError::invalid_request("Object key must be a string")),
            };

            // Validate key format (no leading/trailing whitespace, reasonable length)
            let trimmed_key = sanitized_key.trim();
            if trimmed_key.is_empty() {
                return Err(McpError::invalid_request("Object key cannot be empty"));
            }
            if trimmed_key.len() > 256 {
                return Err(McpError::invalid_request("Object key too long (max 256 chars)"));
            }

            // Sanitize the value
            let sanitized_value = self.sanitize_value(value, depth + 1)?;
            sanitized_obj.insert(trimmed_key.to_string(), sanitized_value);
        }

        Ok(Value::Object(sanitized_obj))
    }

    /// Validate file paths to prevent directory traversal attacks
    pub fn validate_file_path(&self, path: &str) -> McpResult<String> {
        // Sanitize the path string first
        let sanitized_path = match self.sanitize_string(path)? {
            Value::String(s) => s,
            _ => return Err(McpError::invalid_request("Path must be a string")),
        };

        // Check for directory traversal attempts
        if sanitized_path.contains("..") {
            return Err(McpError::invalid_request(
                "Path cannot contain '..' (directory traversal not allowed)",
            ));
        }

        if sanitized_path.starts_with('/') && !sanitized_path.starts_with("/tmp/") {
            return Err(McpError::invalid_request("Absolute paths not allowed (except /tmp/)"));
        }

        // Check for suspicious characters
        if sanitized_path.contains('\0') {
            return Err(McpError::invalid_request("Path cannot contain null bytes"));
        }

        // Limit path length
        if sanitized_path.len() > 1024 {
            return Err(McpError::invalid_request("Path too long (max 1024 characters)"));
        }

        Ok(sanitized_path)
    }

    /// Validate and sanitize URLs
    pub fn validate_url(&self, url: &str) -> McpResult<String> {
        let sanitized_url = match self.sanitize_string(url)? {
            Value::String(s) => s,
            _ => return Err(McpError::invalid_request("URL must be a string")),
        };

        // Basic URL validation
        if !sanitized_url.starts_with("http://") && !sanitized_url.starts_with("https://") {
            return Err(McpError::invalid_request("URL must start with http:// or https://"));
        }

        // Check for reasonable length
        if sanitized_url.len() > 2048 {
            return Err(McpError::invalid_request("URL too long (max 2048 characters)"));
        }

        // Check for suspicious characters
        if sanitized_url.contains('\0') || sanitized_url.contains('\r') || sanitized_url.contains('\n') {
            return Err(McpError::invalid_request("URL contains invalid characters"));
        }

        Ok(sanitized_url)
    }

    /// Validate numeric ranges
    pub fn validate_number_range(&self, value: f64, min: f64, max: f64, name: &str) -> McpResult<f64> {
        if value < min || value > max {
            return Err(McpError::invalid_request(format!(
                "{} must be between {} and {} (got {})",
                name, min, max, value
            )));
        }

        if !value.is_finite() {
            return Err(McpError::invalid_request(format!(
                "{} must be a finite number (got {})",
                name, value
            )));
        }

        Ok(value)
    }

    /// Validate integer ranges
    pub fn validate_integer_range(&self, value: i64, min: i64, max: i64, name: &str) -> McpResult<i64> {
        if value < min || value > max {
            return Err(McpError::invalid_request(format!(
                "{} must be between {} and {} (got {})",
                name, min, max, value
            )));
        }

        Ok(value)
    }

    /// Validate string against allowed patterns
    pub fn validate_string_pattern(&self, value: &str, pattern: &regex::Regex, name: &str) -> McpResult<String> {
        let sanitized = match self.sanitize_string(value)? {
            Value::String(s) => s,
            _ => return Err(McpError::invalid_request(format!("{} must be a string", name))),
        };

        if !pattern.is_match(&sanitized) {
            return Err(McpError::invalid_request(format!(
                "{} does not match required pattern",
                name
            )));
        }

        Ok(sanitized)
    }
}

/// Middleware function to sanitize all MCP tool inputs
pub fn sanitize_tool_input(input: &Value) -> McpResult<Value> {
    let sanitizer = InputSanitizer::default();
    sanitizer.sanitize_input(input)
}

/// Validation helpers for common patterns
pub struct ValidationPatterns;

impl ValidationPatterns {
    /// Base58 pattern for Tari addresses
    pub fn base58() -> regex::Regex {
        regex::Regex::new(r"^[123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]+$")
            .expect("Valid regex pattern")
    }

    /// Hexadecimal pattern
    pub fn hex() -> regex::Regex {
        regex::Regex::new(r"^[0-9a-fA-F]+$").expect("Valid regex pattern")
    }

    /// Alphanumeric with underscores and hyphens
    pub fn alphanumeric_safe() -> regex::Regex {
        regex::Regex::new(r"^[a-zA-Z0-9_-]+$").expect("Valid regex pattern")
    }

    /// Email pattern (basic)
    pub fn email() -> regex::Regex {
        regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").expect("Valid regex pattern")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_html_entity_sanitization() {
        let sanitizer = InputSanitizer::default();

        let input = json!("AT&amp;T &lt;test&gt; &quot;quote&quot;");
        let result = sanitizer.sanitize_input(&input).unwrap();

        assert_eq!(result, json!("AT&T <test> \"quote\""));
    }

    #[test]
    fn test_depth_limit() {
        let sanitizer = InputSanitizer::new(1000, 100, 2);

        // This should exceed the depth limit
        let deep_object = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": "too deep"
                    }
                }
            }
        });

        let result = sanitizer.sanitize_input(&deep_object);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nesting too deep"));
    }

    #[test]
    fn test_string_length_limit() {
        let sanitizer = InputSanitizer::new(10, 100, 10);

        let long_string = json!("This string is definitely longer than 10 characters");
        let result = sanitizer.sanitize_input(&long_string);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("String too long"));
    }

    #[test]
    fn test_array_length_limit() {
        let sanitizer = InputSanitizer::new(1000, 3, 10);

        let long_array = json!([1, 2, 3, 4, 5]);
        let result = sanitizer.sanitize_input(&long_array);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Array too long"));
    }

    #[test]
    fn test_path_validation() {
        let sanitizer = InputSanitizer::default();

        // Valid paths
        assert!(sanitizer.validate_file_path("config.toml").is_ok());
        assert!(sanitizer.validate_file_path("/tmp/test.txt").is_ok());

        // Invalid paths
        assert!(sanitizer.validate_file_path("../../../etc/passwd").is_err());
        assert!(sanitizer.validate_file_path("/etc/passwd").is_err());
        assert!(sanitizer.validate_file_path("file\0name").is_err());
    }

    #[test]
    fn test_url_validation() {
        let sanitizer = InputSanitizer::default();

        // Valid URLs
        assert!(sanitizer.validate_url("https://example.com").is_ok());
        assert!(sanitizer.validate_url("http://localhost:8080/api").is_ok());

        // Invalid URLs
        assert!(sanitizer.validate_url("javascript:alert(1)").is_err());
        assert!(sanitizer.validate_url("file:///etc/passwd").is_err());
        assert!(sanitizer.validate_url("https://example.com\nmalicious").is_err());
    }

    #[test]
    fn test_number_range_validation() {
        let sanitizer = InputSanitizer::default();

        assert!(sanitizer.validate_number_range(5.0, 1.0, 10.0, "test").is_ok());
        assert!(sanitizer.validate_number_range(0.5, 1.0, 10.0, "test").is_err());
        assert!(sanitizer.validate_number_range(15.0, 1.0, 10.0, "test").is_err());
        assert!(sanitizer.validate_number_range(f64::NAN, 1.0, 10.0, "test").is_err());
        assert!(sanitizer
            .validate_number_range(f64::INFINITY, 1.0, 10.0, "test")
            .is_err());
    }

    #[test]
    fn test_pattern_validation() {
        let sanitizer = InputSanitizer::default();
        let hex_pattern = ValidationPatterns::hex();

        assert!(sanitizer
            .validate_string_pattern("deadbeef", &hex_pattern, "hex")
            .is_ok());
        assert!(sanitizer
            .validate_string_pattern("DEADBEEF", &hex_pattern, "hex")
            .is_ok());
        assert!(sanitizer
            .validate_string_pattern("not_hex", &hex_pattern, "hex")
            .is_err());
    }
}
