//! Address-related MCP tools for wallet operations
//!
//! This module provides comprehensive address management including
//! address generation, validation, and format conversion.

use minotari_mcp_common::{McpTool, McpError, McpResult, get_required_string_param, get_optional_string_param};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{Value, json};
use std::sync::Arc;
use tonic::transport::Channel;
use tonic::Request;
use minotari_app_grpc::tari_rpc::{
    Empty, GetPaymentIdAddressRequest,
};

/// Tool for getting wallet addresses
#[derive(Clone)]
pub struct GetAddressTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetAddressTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetAddressTool {
    fn name(&self) -> &str {
        "get_address"
    }
    
    fn description(&self) -> &str {
        "Retrieves the wallet's default addresses in various formats"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.get_address(request).await
            .map_err(|e| McpError::ToolExecution(format!("Failed to get address: {}", e)))?
            .into_inner();
        
        Ok(json!({
            "addresses": {
                "interactive_address": hex::encode(&response.interactive_address),
                "one_sided_address": hex::encode(&response.one_sided_address),
            },
            "info": {
                "interactive_address_info": "Used for interactive Mimblewimble transactions (default)",
                "one_sided_address_info": "Used for one-sided transactions and stealth payments",
                "format": "Hexadecimal encoding of the address bytes",
            }
        }))
    }
}

/// Tool for getting complete address information
#[derive(Clone)]
pub struct GetCompleteAddressTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetCompleteAddressTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetCompleteAddressTool {
    fn name(&self) -> &str {
        "get_complete_address"
    }
    
    fn description(&self) -> &str {
        "Retrieves complete address information in all available formats (binary, base58, emoji)"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.get_complete_address(request).await
            .map_err(|e| McpError::ToolExecution(format!("Failed to get complete address: {}", e)))?
            .into_inner();
        
        Ok(json!({
            "addresses": {
                "interactive": {
                    "bytes": hex::encode(&response.interactive_address),
                    "base58": response.interactive_address_base58,
                    "emoji": response.interactive_address_emoji,
                },
                "one_sided": {
                    "bytes": hex::encode(&response.one_sided_address),
                    "base58": response.one_sided_address_base58,
                    "emoji": response.one_sided_address_emoji,
                }
            },
            "formats": {
                "bytes": "Raw address bytes in hexadecimal format",
                "base58": "Human-readable Base58 encoded address",
                "emoji": "User-friendly emoji representation for easy sharing",
            },
            "usage_recommendations": {
                "interactive_address": "Use for most transactions - enables interactive negotiation",
                "one_sided_address": "Use for stealth payments or when sender privacy is required",
                "base58_format": "Recommended for sharing with other users",
                "emoji_format": "Fun alternative for casual transactions",
            }
        }))
    }
}

/// Tool for generating payment-specific addresses
#[derive(Clone)]
pub struct GetPaymentIdAddressTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetPaymentIdAddressTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetPaymentIdAddressTool {
    fn name(&self) -> &str {
        "get_payment_id_address"
    }
    
    fn description(&self) -> &str {
        "Generates addresses for a specific payment ID, useful for tracking payments"
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let payment_id_str = get_required_string_param(&params, "payment_id")?;
        
        let payment_id = if payment_id_str.starts_with("0x") {
            hex::decode(&payment_id_str[2..])
                .map_err(|e| McpError::InvalidParameter(format!("Invalid hex payment ID: {}", e)))?
        } else {
            payment_id_str.as_bytes().to_vec()
        };
        
        let request = Request::new(GetPaymentIdAddressRequest { payment_id });
        
        let response = self.grpc_client.get_payment_id_address(request).await
            .map_err(|e| McpError::ToolExecution(format!("Failed to get payment ID address: {}", e)))?
            .into_inner();
        
        Ok(json!({
            "payment_id": payment_id_str,
            "addresses": {
                "interactive": {
                    "bytes": hex::encode(&response.interactive_address),
                    "base58": response.interactive_address_base58,
                    "emoji": response.interactive_address_emoji,
                },
                "one_sided": {
                    "bytes": hex::encode(&response.one_sided_address),
                    "base58": response.one_sided_address_base58,
                    "emoji": response.one_sided_address_emoji,
                }
            },
            "info": {
                "purpose": "These addresses are unique to the specified payment ID",
                "tracking": "Payments to these addresses will be associated with the payment ID",
                "privacy": "Each payment ID generates unique addresses for better privacy",
            },
            "usage": {
                "invoicing": "Use these addresses for specific invoices or payment requests",
                "reconciliation": "Easier to track and reconcile payments from different sources",
                "reporting": "Simplifies accounting and transaction categorization",
            }
        }))
    }
}

/// Tool for address validation and analysis
#[derive(Clone)]
pub struct AddressValidationTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl AddressValidationTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for AddressValidationTool {
    fn name(&self) -> &str {
        "validate_address"
    }
    
    fn description(&self) -> &str {
        "Validates and analyzes Tari addresses for format, type, and usability"
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let address = get_required_string_param(&params, "address")?;
        
        // Basic validation checks
        let mut validation_results = Vec::new();
        let mut is_valid = true;
        let mut address_type = "UNKNOWN";
        let mut format_type = "UNKNOWN";
        
        // Check if it's a hex address
        if address.starts_with("0x") || address.chars().all(|c| c.is_ascii_hexdigit()) {
            let hex_addr = if address.starts_with("0x") { &address[2..] } else { &address };
            
            match hex::decode(hex_addr) {
                Ok(bytes) => {
                    format_type = "HEX";
                    match bytes.len() {
                        33 => {
                            address_type = "INTERACTIVE";
                            validation_results.push("Valid interactive address length (33 bytes)".to_string());
                        },
                        32 => {
                            address_type = "ONE_SIDED";
                            validation_results.push("Valid one-sided address length (32 bytes)".to_string());
                        },
                        _ => {
                            is_valid = false;
                            validation_results.push(format!("Invalid address length: {} bytes (expected 32 or 33)", bytes.len()));
                        }
                    }
                },
                Err(_) => {
                    is_valid = false;
                    validation_results.push("Invalid hexadecimal format".to_string());
                }
            }
        }
        // Check if it's a Base58 address (Tari addresses typically start with specific characters)
        else if address.len() > 40 && address.chars().all(|c| {
            "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c)
        }) {
            format_type = "BASE58";
            address_type = "INTERACTIVE"; // Most Base58 addresses are interactive
            validation_results.push("Valid Base58 format detected".to_string());
            
            // Additional Base58 validation could be added here
            if address.len() < 44 || address.len() > 88 {
                validation_results.push("Warning: Base58 address length seems unusual".to_string());
            }
        }
        // Check if it's an emoji address
        else if address.chars().any(|c| c as u32 > 127) {
            format_type = "EMOJI";
            address_type = "INTERACTIVE";
            validation_results.push("Emoji address format detected".to_string());
            
            let emoji_count = address.chars().filter(|c| c as u32 > 127).count();
            if emoji_count < 8 {
                validation_results.push("Warning: Emoji address seems too short".to_string());
            }
        }
        else {
            is_valid = false;
            validation_results.push("Unrecognized address format".to_string());
        }
        
        // Generate recommendations
        let mut recommendations = Vec::new();
        
        if !is_valid {
            recommendations.push("Verify the address was copied correctly".to_string());
            recommendations.push("Check with the sender for the correct address format".to_string());
        } else {
            recommendations.push("Address format appears valid".to_string());
            
            match format_type {
                "HEX" => recommendations.push("Hex addresses are valid but Base58 format is more user-friendly".to_string()),
                "BASE58" => recommendations.push("Base58 format is recommended for sharing with users".to_string()),
                "EMOJI" => recommendations.push("Emoji format is fun but verify carefully before sending funds".to_string()),
                _ => {}
            }
            
            if address_type == "INTERACTIVE" {
                recommendations.push("Interactive addresses support full Mimblewimble protocol features".to_string());
            } else if address_type == "ONE_SIDED" {
                recommendations.push("One-sided addresses provide enhanced sender privacy".to_string());
            }
        }
        
        Ok(json!({
            "address": address,
            "validation": {
                "is_valid": is_valid,
                "address_type": address_type,
                "format_type": format_type,
                "checks_performed": validation_results,
            },
            "analysis": {
                "length": address.len(),
                "character_count": address.chars().count(),
                "contains_special_chars": address.chars().any(|c| !c.is_alphanumeric()),
                "estimated_type": match (format_type, address_type) {
                    ("BASE58", "INTERACTIVE") => "Standard interactive Tari address",
                    ("HEX", "INTERACTIVE") => "Raw interactive address bytes",
                    ("HEX", "ONE_SIDED") => "Raw one-sided address bytes",
                    ("EMOJI", _) => "Emoji-encoded Tari address",
                    _ => "Unknown or invalid address type",
                }
            },
            "recommendations": recommendations,
            "usage_notes": {
                "sending_funds": if is_valid {
                    "Address appears valid for receiving Tari"
                } else {
                    "Do not send funds to this address - validation failed"
                },
                "preferred_format": "Base58 format is recommended for most users",
                "verification": "Always verify addresses with the recipient before sending funds",
            }
        }))
    }
}

/// Tool for address format conversion
#[derive(Clone)]
pub struct AddressConverterTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl AddressConverterTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for AddressConverterTool {
    fn name(&self) -> &str {
        "convert_address_format"
    }
    
    fn description(&self) -> &str {
        "Converts Tari addresses between different formats (hex, base58, emoji)"
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let address = get_required_string_param(&params, "address")?;
        let target_format = get_optional_string_param(&params, "target_format")?.unwrap_or("all".to_string());
        
        // This is a simplified conversion - in a real implementation, we'd need
        // proper address parsing and conversion logic
        
        let mut conversions = json!({});
        let mut errors = Vec::new();
        
        // Try to determine the input format and convert
        if address.starts_with("0x") || address.chars().all(|c| c.is_ascii_hexdigit()) {
            // Input is hex format
            let hex_addr = if address.starts_with("0x") { &address[2..] } else { &address };
            
            match hex::decode(hex_addr) {
                Ok(_bytes) => {
                    conversions["hex"] = json!(format!("0x{}", hex_addr));
                    conversions["original_format"] = json!("hex");
                    
                    // Note: Real conversion to Base58 and emoji would require
                    // proper Tari address encoding libraries
                    conversions["note"] = json!("Full format conversion requires Tari address encoding libraries");
                },
                Err(e) => {
                    errors.push(format!("Invalid hex format: {}", e));
                }
            }
        } else {
            errors.push("Address format conversion is limited in this implementation".to_string());
            errors.push("Contact the wallet provider for full address conversion support".to_string());
        }
        
        Ok(json!({
            "original_address": address,
            "target_format": target_format,
            "conversions": conversions,
            "errors": errors,
            "note": "This is a simplified address converter. Full conversion requires additional Tari libraries.",
            "recommendations": [
                "Use the wallet's built-in address display for accurate format conversion",
                "The get_complete_address tool provides all formats for your own addresses",
                "Always verify converted addresses before using them for transactions"
            ]
        }))
    }
}
