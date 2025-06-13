//! Stdio-based MCP transport following JSON-RPC 2.0 specification
//! 
//! This module implements the Model Context Protocol (MCP) stdio transport
//! which uses standard input/output streams for communication. All messages
//! are JSON-RPC 2.0 formatted and line-delimited. Logs go to stderr only.

use crate::error::{McpError, McpResult};
use crate::transport::{McpMessage, McpResponse, MessageHandler};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::signal;
use tokio::sync::mpsc;

/// Stdio transport implementation for MCP
pub struct StdioTransport {
    message_handler: Arc<dyn MessageHandler>,
}

impl StdioTransport {
    pub fn new(message_handler: Arc<dyn MessageHandler>) -> Self {
        Self { message_handler }
    }

    /// Start the stdio transport
    pub async fn start(&self) -> McpResult<()> {
        // Setup graceful shutdown
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        
        // Handle SIGINT and SIGTERM for graceful shutdown
        let shutdown_tx_clone = shutdown_tx.clone();
        tokio::spawn(async move {
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
                .expect("Failed to install SIGINT handler");
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            
            tokio::select! {
                _ = sigint.recv() => {
                    log::info!("Received SIGINT, shutting down gracefully");
                }
                _ = sigterm.recv() => {
                    log::info!("Received SIGTERM, shutting down gracefully");
                }
            }
            let _ = shutdown_tx_clone.send(());
        });

        // Setup stdin reader and stdout writer
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut writer = BufWriter::new(stdout);
        let mut line = String::new();

        log::info!("MCP stdio transport started");

        loop {
            line.clear();
            
            tokio::select! {
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    log::info!("Shutdown signal received, terminating");
                    break;
                }
                
                // Read from stdin
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => {
                            // EOF - client disconnected
                            log::info!("Client disconnected (EOF)");
                            break;
                        }
                        Ok(_) => {
                            let line_content = line.trim();
                            if line_content.is_empty() {
                                continue;
                            }

                            log::debug!("Received message: {}", line_content);

                            // Parse and handle the message
                            let response = self.handle_json_message(line_content).await;
                            
                            // Send response if there is one
                            if let Some(response_json) = response {
                                if let Err(e) = writer.write_all(response_json.as_bytes()).await {
                                    log::error!("Failed to write response: {}", e);
                                    break;
                                }
                                if let Err(e) = writer.write_all(b"\n").await {
                                    log::error!("Failed to write newline: {}", e);
                                    break;
                                }
                                if let Err(e) = writer.flush().await {
                                    log::error!("Failed to flush output: {}", e);
                                    break;
                                }
                                
                                log::debug!("Sent response: {}", response_json);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to read from stdin: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        log::info!("MCP stdio transport stopped");
        Ok(())
    }

    /// Handle a JSON message with robust parsing and error recovery
    async fn handle_json_message(&self, line: &str) -> Option<String> {
        // First try to parse the JSON as-is
        let message_result = match serde_json::from_str::<McpMessage>(line) {
            Ok(message) => Ok(message),
            Err(_) => {
                // If parsing fails, try to repair the JSON
                log::debug!("Initial JSON parse failed, attempting repair");
                match self.repair_and_parse_json(line) {
                    Some(repaired) => {
                        log::debug!("JSON repair successful: {}", repaired);
                        serde_json::from_str::<McpMessage>(&repaired)
                            .map_err(|e| McpError::invalid_request(format!("JSON repair failed: {}", e)))
                    }
                    None => {
                        Err(McpError::invalid_request("Unable to repair malformed JSON"))
                    }
                }
            }
        };

        // Handle the message
        let response = match message_result {
            Ok(message) => {
                self.message_handler.handle_message(message).await
            }
            Err(e) => {
                log::warn!("Message parsing failed: {}", e);
                Err(e)
            }
        };

        // Convert response to JSON
        let response_json = match response {
            Ok(resp) => {
                match serde_json::to_string(&resp) {
                    Ok(json) => json,
                    Err(e) => {
                        log::error!("Failed to serialize response: {}", e);
                        self.create_error_response(Value::Null, -32603, "Internal error: serialization failed")
                    }
                }
            }
            Err(e) => {
                let error_code = match &e {
                    McpError::PermissionDenied(_) => -32000,
                    McpError::InvalidRequest(_) => -32600,
                    McpError::ToolNotFound(_) | McpError::ResourceNotFound(_) => -32601,
                    _ => -32603,
                };
                self.create_error_response(Value::Null, error_code, &e.to_string())
            }
        };

        Some(response_json)
    }

    /// Attempt to repair common JSON malformations from AI agents
    fn repair_and_parse_json(&self, input: &str) -> Option<String> {
        let mut repaired = input.to_string();

        // 1. Fix unquoted property names
        // Pattern: {word: or ,word: -> {"word": or ,"word":
        let unquoted_keys = regex::Regex::new(r#"([{,]\s*)(\w+)(\s*:)"#).ok()?;
        repaired = unquoted_keys.replace_all(&repaired, r#"$1"$2"$3"#).to_string();

        // 2. Remove trailing commas
        // Pattern: , followed by } or ]
        let trailing_commas = regex::Regex::new(r#",\s*([}\]])"#).ok()?;
        repaired = trailing_commas.replace_all(&repaired, "$1").to_string();

        // 3. Fix unquoted string values (but not numbers, booleans, null)
        // This is tricky - look for values that should be quoted
        let unquoted_values = regex::Regex::new(r#":\s*([^{"\[\d\s][\w\s]*[^,}\]]*)"#).ok()?;
        repaired = unquoted_values.replace_all(&repaired, r#": "$1""#).to_string();

        // 4. Fix HTML entities
        repaired = repaired
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'");

        // 5. Replace single quotes with double quotes for strings
        let single_quotes = regex::Regex::new(r#"'([^']*)'"#).ok()?;
        repaired = single_quotes.replace_all(&repaired, r#""$1""#).to_string();

        // 6. Remove comments (// or /* */)
        let comments = regex::Regex::new(r#"(//.*|/\*[\s\S]*?\*/)"#).ok()?;
        repaired = comments.replace_all(&repaired, "").to_string();

        // 7. Fix leading zeros in numbers
        let leading_zeros = regex::Regex::new(r#"(?<!\d)0+(\d+)"#).ok()?;
        repaired = leading_zeros.replace_all(&repaired, "$1").to_string();

        // Return the repaired JSON if it's different from input
        if repaired != input {
            Some(repaired)
        } else {
            None
        }
    }

    /// Create a JSON-RPC error response
    fn create_error_response(&self, id: Value, code: i32, message: &str) -> String {
        let error_response = McpResponse {
            id,
            result: None,
            error: Some(crate::transport::McpErrorResponse {
                code,
                message: message.to_string(),
                data: None,
            }),
        };
        
        serde_json::to_string(&error_response)
            .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{MessageHandler, McpMessage};
    use async_trait::async_trait;

    struct TestMessageHandler;

    #[async_trait]
    impl MessageHandler for TestMessageHandler {
        async fn handle_message(&self, _message: McpMessage) -> McpResult<McpResponse> {
            Ok(McpResponse {
                id: serde_json::Value::Number(serde_json::Number::from(1)),
                result: Some(serde_json::json!({"test": "response"})),
                error: None,
            })
        }
    }

    #[tokio::test]
    async fn test_json_repair_unquoted_keys() {
        let transport = StdioTransport::new(Arc::new(TestMessageHandler));
        
        let malformed = r#"{name: "test", id: 123}"#;
        let repaired = transport.repair_and_parse_json(malformed).unwrap();
        assert_eq!(repaired, r#"{"name": "test", "id": 123}"#);
    }

    #[tokio::test]
    async fn test_json_repair_trailing_commas() {
        let transport = StdioTransport::new(Arc::new(TestMessageHandler));
        
        let malformed = r#"{"name": "test", "id": 123,}"#;
        let repaired = transport.repair_and_parse_json(malformed).unwrap();
        assert_eq!(repaired, r#"{"name": "test", "id": 123}"#);
    }

    #[tokio::test]
    async fn test_json_repair_html_entities() {
        let transport = StdioTransport::new(Arc::new(TestMessageHandler));
        
        let malformed = r#"{"message": "AT&amp;T &lt;test&gt;"}"#;
        let repaired = transport.repair_and_parse_json(malformed).unwrap();
        assert_eq!(repaired, r#"{"message": "AT&T <test>"}"#);
    }

    #[tokio::test]
    async fn test_valid_json_unchanged() {
        let transport = StdioTransport::new(Arc::new(TestMessageHandler));
        
        let valid = r#"{"jsonrpc": "2.0", "id": 1, "method": "test"}"#;
        let result = transport.repair_and_parse_json(valid);
        assert!(result.is_none()); // Valid JSON should not be "repaired"
    }
}
