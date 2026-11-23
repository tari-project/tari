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

/// Longest accepted hex string for an encoded block, block template or transaction: 4 MiB of protobuf. A block at
/// the mainnet maximum weight of 127,795 grams is about 2-2.5 MiB (16-20 bytes per gram), so this is roughly 2x that,
/// and it still fits the node's 8 MiB gRPC message limit.
pub const MAX_ENCODED_BLOCK_HEX_LEN: usize = 8 * 1024 * 1024;

/// Fields that carry a hex-encoded block, block template or transaction and so get [MAX_ENCODED_BLOCK_HEX_LEN]
/// instead of the general string limit
const ENCODED_BLOCK_HEX_FIELDS: [&str; 3] = ["block_hex", "transaction_hex", "new_block_template_hex"];

/// Fields that reach the chain or a wallet query as written, so they are checked but never rewritten: a payment id
/// that would be changed is refused instead of being silently altered
const VERBATIM_FIELDS: [&str; 1] = ["payment_id"];

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
        self.sanitize_string_with_limit(s, self.max_string_length)
    }

    /// Accept `s` unchanged, or refuse it if sanitizing would change it
    fn check_verbatim(&self, key: &str, s: &str) -> McpResult<Value> {
        if s.len() > self.max_string_length {
            return Err(McpError::invalid_request(format!(
                "{key} too long ({} chars, max {})",
                s.len(),
                self.max_string_length
            )));
        }
        if s.chars().any(char::is_control) || !s.nfc().eq(s.chars()) {
            return Err(McpError::invalid_request(format!(
                "{key} must not contain control characters and must be NFC-normalized Unicode"
            )));
        }
        Ok(Value::String(s.to_string()))
    }

    fn sanitize_string_with_limit(&self, s: &str, max_len: usize) -> McpResult<Value> {
        // Check length limit
        if s.len() > max_len {
            return Err(McpError::invalid_request(format!(
                "String too long ({} chars, max {max_len})",
                s.len()
            )));
        }

        // Decode HTML entities
        let mut sanitized = s.to_string();
        for (entity, replacement) in &self.allowed_html_entities {
            sanitized = sanitized.replace(entity, replacement);
        }
        // Decode &amp; last so double-encoded entities (e.g. &amp;lt;) are not decoded into < or >
        if !self.allowed_html_entities.is_empty() {
            sanitized = sanitized.replace("&amp;", "&");
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
            let sanitized_item = self.sanitize_value(item, depth.saturating_add(1))?;
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

        // shortcut: a key repeated in the raw JSON is collapsed by serde_json (last one wins) before it gets here;
        // rejecting that needs a duplicate-aware deserializer in the transport.
        let mut sanitized_obj = serde_json::Map::new();
        for (key, value) in obj {
            // Sanitize the key (treat as string)
            let sanitized_key = match self.sanitize_string(key)? {
                Value::String(s) => s,
                _ => return Err(McpError::invalid_request("Object key must be a string")),
            };

            // Refuse a key that cleaning would change rather than rename it: a lookalike such as "amount " could
            // otherwise replace the real key. Unchanged keys are distinct, so no two keys can merge.
            let trimmed_key = sanitized_key.trim();
            if trimmed_key != key {
                return Err(McpError::invalid_request(format!(
                    "Object key {key:?} contains control characters, HTML entities, surrounding whitespace or non-NFC \
                     text"
                )));
            }
            if trimmed_key.is_empty() {
                return Err(McpError::invalid_request("Object key cannot be empty"));
            }
            if trimmed_key.len() > 256 {
                return Err(McpError::invalid_request("Object key too long (max 256 chars)"));
            }

            // Sanitize the value
            let sanitized_value = match value {
                Value::String(s) if ENCODED_BLOCK_HEX_FIELDS.contains(&trimmed_key) => {
                    self.sanitize_string_with_limit(s, MAX_ENCODED_BLOCK_HEX_LEN)?
                },
                Value::String(s) if VERBATIM_FIELDS.contains(&trimmed_key) => self.check_verbatim(trimmed_key, s)?,
                _ => self.sanitize_value(value, depth.saturating_add(1))?,
            };
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
                "{name} must be between {min} and {max} (got {value})"
            )));
        }

        if !value.is_finite() {
            return Err(McpError::invalid_request(format!(
                "{name} must be a finite number (got {value})"
            )));
        }

        Ok(value)
    }

    /// Validate integer ranges
    pub fn validate_integer_range(&self, value: i64, min: i64, max: i64, name: &str) -> McpResult<i64> {
        if value < min || value > max {
            return Err(McpError::invalid_request(format!(
                "{name} must be between {min} and {max} (got {value})"
            )));
        }

        Ok(value)
    }

    /// Validate string against allowed patterns
    pub fn validate_string_pattern(&self, value: &str, pattern: &regex::Regex, name: &str) -> McpResult<String> {
        let sanitized = match self.sanitize_string(value)? {
            Value::String(s) => s,
            _ => return Err(McpError::invalid_request(format!("{name} must be a string"))),
        };

        if !pattern.is_match(&sanitized) {
            return Err(McpError::invalid_request(format!(
                "{name} does not match required pattern"
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
    #![allow(clippy::indexing_slicing)]
    use serde_json::json;

    use super::*;

    #[test]
    fn keys_that_cleaning_would_change_are_refused() {
        assert!(sanitize_tool_input(&json!({ "amount": 1, "amount\u{007f}": 2 })).is_err());
        let err = sanitize_tool_input(&json!({ "address ": "x" }))
            .unwrap_err()
            .to_string();
        assert!(err.contains(r#""address ""#), "{err}");
        assert!(sanitize_tool_input(&json!({ "recipients": [{ "address ": "x", "amount": 1 }] })).is_err());
        assert!(sanitize_tool_input(&json!({ "a&amp;b": 1 })).is_err());

        let normal = json!({ "recipients": [{ "address": "x", "amount": 1 }], "fee_per_gram": 5 });
        assert_eq!(sanitize_tool_input(&normal).unwrap(), normal);
    }

    #[test]
    fn payment_id_is_kept_verbatim_or_refused() {
        let kept = sanitize_tool_input(&json!({ "recipients": [{ "payment_id": "inv &amp; <7> é" }] })).unwrap();
        assert_eq!(kept["recipients"][0]["payment_id"], "inv &amp; <7> é");
        assert!(sanitize_tool_input(&json!({ "payment_id": "line\nbreak" })).is_err());
        assert!(
            sanitize_tool_input(&json!({ "payment_id": "e\u{301}" })).is_err(),
            "not NFC"
        );
        // Other fields are still decoded
        assert_eq!(sanitize_tool_input(&json!({ "memo": "&amp;" })).unwrap()["memo"], "&");
    }

    #[test]
    fn encoded_block_hex_fields_get_their_own_limit() {
        let large = "ab".repeat(64 * 1024);
        for key in ENCODED_BLOCK_HEX_FIELDS {
            assert!(sanitize_tool_input(&json!({ key: large })).is_ok());
            let over = "a".repeat(MAX_ENCODED_BLOCK_HEX_LEN + 1);
            assert!(sanitize_tool_input(&json!({ key: over })).is_err());
        }
        // Every other field keeps the general limit
        assert!(sanitize_tool_input(&json!({ "memo": large })).is_err());
    }

    #[test]
    fn test_html_entity_sanitization() {
        let sanitizer = InputSanitizer::default();

        let input = json!("AT&amp;T &lt;test&gt; &quot;quote&quot; &amp;lt;escaped&amp;gt;");
        let result = sanitizer.sanitize_input(&input).unwrap();

        assert_eq!(result, json!("AT&T <test> \"quote\" &lt;escaped&gt;"));
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
        assert!(
            sanitizer
                .validate_number_range(f64::INFINITY, 1.0, 10.0, "test")
                .is_err()
        );
    }

    #[test]
    fn test_pattern_validation() {
        let sanitizer = InputSanitizer::default();
        let hex_pattern = ValidationPatterns::hex();

        assert!(
            sanitizer
                .validate_string_pattern("deadbeef", &hex_pattern, "hex")
                .is_ok()
        );
        assert!(
            sanitizer
                .validate_string_pattern("DEADBEEF", &hex_pattern, "hex")
                .is_ok()
        );
        assert!(
            sanitizer
                .validate_string_pattern("not_hex", &hex_pattern, "hex")
                .is_err()
        );
    }
}
