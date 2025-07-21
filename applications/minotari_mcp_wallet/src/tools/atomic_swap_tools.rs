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
//! Atomic swap MCP tools for wallet operations
//!
//! This module provides tools for atomic swap transactions including
//! SHA atomic swaps, HTLC operations, and swap management.

use std::sync::Arc;

use minotari_app_grpc::tari_rpc::{
    ClaimHtlcRefundRequest,
    ClaimShaAtomicSwapRequest,
    PaymentRecipient,
    SendShaAtomicSwapRequest,
};
use minotari_mcp_common::{
    get_required_string_param,
    get_required_u64_param,
    McpError,
    McpResult,
    McpTool,
    PermissionLevel,
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{json, Value};
use tonic::{transport::Channel, Request};

/// Tool for sending SHA atomic swap transactions
#[derive(Clone)]
pub struct SendShaAtomicSwapTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl SendShaAtomicSwapTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for SendShaAtomicSwapTool {
    fn name(&self) -> &str {
        "send_sha_atomic_swap"
    }

    fn description(&self) -> &str {
        "Initiates a SHA-based atomic swap transaction with hash time-locked contract"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "recipient": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Recipient address for the atomic swap"
                        },
                        "amount": {
                            "type": "number",
                            "description": "Amount in microTari to send"
                        },
                        "fee_per_gram": {
                            "type": "number",
                            "description": "Fee per gram in microTari"
                        }
                    },
                    "required": ["address", "amount", "fee_per_gram"],
                    "description": "Recipient details for the atomic swap"
                }
            },
            "required": ["recipient"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let recipient_data = params
            .get("recipient")
            .ok_or_else(|| McpError::invalid_request("recipient object is required"))?;

        let address = recipient_data
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_request("recipient.address is required"))?;

        let amount = recipient_data
            .get("amount")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| McpError::invalid_request("recipient.amount is required"))?;

        let fee_per_gram = recipient_data
            .get("fee_per_gram")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| McpError::invalid_request("recipient.fee_per_gram is required"))?;

        if amount == 0 {
            return Err(McpError::invalid_request("amount must be greater than 0"));
        }

        if fee_per_gram == 0 {
            return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
        }

        let recipient = PaymentRecipient {
            address: address.to_string(),
            amount,
            fee_per_gram,
            payment_type: 2, // ONE_SIDED_TO_STEALTH_ADDRESS
            raw_payment_id: vec![],
            user_payment_id: None,
        };

        let request = Request::new(SendShaAtomicSwapRequest {
            recipient: Some(recipient),
        });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .send_sha_atomic_swap_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to send SHA atomic swap: {}", e)))?
            .into_inner();

        Ok(json!({
            "transaction_id": response.transaction_id,
            "pre_image": hex::encode(&response.pre_image),
            "output_hash": hex::encode(&response.output_hash),
            "is_success": response.is_success,
            "failure_message": response.failure_message,
            "swap_details": {
                "recipient_address": address,
                "amount": amount,
                "fee_per_gram": fee_per_gram,
                "amount_tari": (amount as f64 / 1_000_000.0),
            },
            "security_info": {
                "pre_image_purpose": "Keep this pre-image secret until you want to claim the swap",
                "output_hash_usage": "The recipient needs this hash to identify the swap output",
                "claiming_process": "To claim the swap, the recipient must provide the pre-image that matches this hash",
            },
            "next_steps": if response.is_success {
                vec![
                    "Share the output_hash with the recipient".to_string(),
                    "Keep the pre_image secret until ready to reveal".to_string(),
                    "Monitor the transaction for confirmation".to_string(),
                    "The recipient can claim using the pre_image once revealed".to_string()
                ]
            } else {
                vec![
                    format!("Fix the issue: {}", response.failure_message),
                    "Check wallet balance and network connectivity".to_string(),
                    "Retry the atomic swap operation".to_string()
                ]
            }
        }))
    }
}

/// Tool for claiming SHA atomic swap transactions
#[derive(Clone)]
pub struct ClaimShaAtomicSwapTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl ClaimShaAtomicSwapTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for ClaimShaAtomicSwapTool {
    fn name(&self) -> &str {
        "claim_sha_atomic_swap"
    }

    fn description(&self) -> &str {
        "Claims a SHA atomic swap by providing the pre-image that matches the output hash"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "output_hash": {
                    "type": "string",
                    "description": "Output hash of the atomic swap to claim (64 hex characters)"
                },
                "pre_image": {
                    "type": "string",
                    "description": "Pre-image that matches the output hash (hex string)"
                },
                "fee_per_gram": {
                    "type": "number",
                    "description": "Fee per gram in microTari"
                }
            },
            "required": ["output_hash", "pre_image", "fee_per_gram"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let output_hash = get_required_string_param(&params, "output_hash")?;
        let pre_image = get_required_string_param(&params, "pre_image")?;
        let fee_per_gram = get_required_u64_param(&params, "fee_per_gram")?;

        // Validate hex inputs
        if !output_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("output_hash must be a valid hex string"));
        }

        if !pre_image.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("pre_image must be a valid hex string"));
        }

        if output_hash.len() != 64 {
            return Err(McpError::invalid_request(
                "output_hash must be 64 hex characters (32 bytes)",
            ));
        }

        if fee_per_gram == 0 {
            return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
        }

        let request = Request::new(ClaimShaAtomicSwapRequest {
            output: output_hash.clone(),
            pre_image: pre_image.clone(),
            fee_per_gram,
        });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .claim_sha_atomic_swap_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to claim SHA atomic swap: {}", e)))?
            .into_inner();

        let result = response
            .results
            .as_ref()
            .ok_or_else(|| McpError::tool_execution_failed("No results in claim response"))?;

        Ok(json!({
            "claim_result": {
                "address": result.address,
                "transaction_id": result.transaction_id,
                "is_success": result.is_success,
                "failure_message": result.failure_message,
            },
            "claim_details": {
                "output_hash": output_hash,
                "pre_image_provided": pre_image,
                "fee_per_gram": fee_per_gram,
            },
            "status": if result.is_success {
                "CLAIMED_SUCCESSFULLY"
            } else {
                "CLAIM_FAILED"
            },
            "message": if result.is_success {
                "Atomic swap has been successfully claimed".to_string()
            } else {
                format!("Claim failed: {}", result.failure_message)
            },
            "security_notes": {
                "pre_image_revealed": "The pre-image is now publicly visible on the blockchain",
                "transaction_finality": "This claim transaction must be confirmed to complete the swap",
                "fund_availability": "Claimed funds will be available once the transaction confirms",
            },
            "next_steps": if result.is_success {
                vec![
                    "Wait for transaction confirmation",
                    "Funds will appear in wallet balance",
                    "Atomic swap is now complete"
                ]
            } else {
                vec![
                    "Verify the pre-image matches the expected hash",
                    "Check that the output hasn't already been claimed",
                    "Ensure sufficient funds for transaction fee",
                    "Retry with correct parameters"
                ]
            }
        }))
    }
}

/// Tool for claiming HTLC refunds
#[derive(Clone)]
pub struct ClaimHtlcRefundTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl ClaimHtlcRefundTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for ClaimHtlcRefundTool {
    fn name(&self) -> &str {
        "claim_htlc_refund"
    }

    fn description(&self) -> &str {
        "Claims a refund for an expired HTLC (Hash Time-Locked Contract) after the timelock period"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "output_hash": {
                    "type": "string",
                    "description": "Output hash of the HTLC to refund (64 hex characters)"
                },
                "fee_per_gram": {
                    "type": "number",
                    "description": "Fee per gram in microTari"
                }
            },
            "required": ["output_hash", "fee_per_gram"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let output_hash = get_required_string_param(&params, "output_hash")?;
        let fee_per_gram = get_required_u64_param(&params, "fee_per_gram")?;

        // Validate hex input
        if !output_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("output_hash must be a valid hex string"));
        }

        if output_hash.len() != 64 {
            return Err(McpError::invalid_request(
                "output_hash must be 64 hex characters (32 bytes)",
            ));
        }

        if fee_per_gram == 0 {
            return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
        }

        let request = Request::new(ClaimHtlcRefundRequest {
            output_hash: output_hash.clone(),
            fee_per_gram,
        });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .claim_htlc_refund_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to claim HTLC refund: {}", e)))?
            .into_inner();

        let result = response
            .results
            .as_ref()
            .ok_or_else(|| McpError::tool_execution_failed("No results in refund response"))?;

        Ok(json!({
            "refund_result": {
                "address": result.address,
                "transaction_id": result.transaction_id,
                "is_success": result.is_success,
                "failure_message": result.failure_message,
            },
            "refund_details": {
                "output_hash": output_hash,
                "fee_per_gram": fee_per_gram,
            },
            "status": if result.is_success {
                "REFUND_CLAIMED"
            } else {
                "REFUND_FAILED"
            },
            "message": if result.is_success {
                "HTLC refund has been successfully claimed".to_string()
            } else {
                format!("Refund claim failed: {}", result.failure_message)
            },
            "htlc_info": {
                "purpose": "HTLC refunds allow recovery of funds when atomic swaps timeout",
                "timelock": "Refunds are only possible after the timelock period expires",
                "finality": "This refund transaction must be confirmed to recover the funds",
            },
            "next_steps": if result.is_success {
                vec![
                    "Wait for refund transaction confirmation",
                    "Funds will be returned to wallet balance",
                    "The atomic swap opportunity has expired"
                ]
            } else {
                vec![
                    "Check that the timelock period has expired",
                    "Verify the output hash is correct",
                    "Ensure the HTLC hasn't been claimed by the recipient",
                    "Check wallet balance for transaction fees"
                ]
            }
        }))
    }
}

/// Tool for atomic swap status and management
#[derive(Clone)]
pub struct AtomicSwapStatusTool {
    _grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl AtomicSwapStatusTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self {
            _grpc_client: grpc_client,
        }
    }
}

#[async_trait::async_trait]
impl McpTool for AtomicSwapStatusTool {
    fn name(&self) -> &str {
        "atomic_swap_status"
    }

    fn description(&self) -> &str {
        "Provides status information and guidance for atomic swap operations"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "swap_type": {
                    "type": "string",
                    "description": "Type of swap to get information about",
                    "enum": ["general", "sha", "sha_atomic_swap", "htlc", "htlc_refund"]
                },
                "output_hash": {
                    "type": "string",
                    "description": "Optional output hash to check status for (64 hex characters)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let swap_type = params.get("swap_type").and_then(|v| v.as_str()).unwrap_or("general");

        let output_hash = params.get("output_hash").and_then(|v| v.as_str());

        // Generate general atomic swap guidance
        let mut info = json!({
            "atomic_swap_overview": {
                "purpose": "Atomic swaps enable trustless exchange of assets between parties",
                "security": "Cryptographic guarantees ensure either both parties get their assets or neither does",
                "use_cases": [
                    "Cross-chain asset exchanges",
                    "Trustless trading without intermediaries",
                    "Conditional payments with automatic refunds"
                ]
            },
            "swap_types": {
                "sha_atomic_swap": {
                    "description": "Uses SHA-256 hash locks for secure asset exchange",
                    "process": "Sender creates HTLC → Recipient claims with pre-image → Swap completes",
                    "security": "Pre-image must be kept secret until ready to claim",
                },
                "htlc_refund": {
                    "description": "Recovery mechanism for expired HTLCs",
                    "purpose": "Allows original sender to recover funds if recipient doesn't claim",
                    "timing": "Only available after the timelock period expires",
                }
            },
            "best_practices": {
                "security": [
                    "Keep pre-images secret until ready to reveal",
                    "Verify all addresses before initiating swaps",
                    "Use appropriate timelock periods",
                    "Monitor swap status regularly"
                ],
                "operational": [
                    "Test with small amounts first",
                    "Ensure sufficient funds for fees",
                    "Plan for network confirmation times",
                    "Have backup plans for failed swaps"
                ]
            }
        });

        // Add specific information if output hash is provided
        if let Some(hash) = output_hash {
            if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                info["specific_swap"] = json!({
                    "output_hash": hash,
                    "status_check": "Use blockchain explorer to check if this output has been spent",
                    "claiming": "If unspent, check timelock status to determine claim eligibility",
                    "monitoring": "Track this hash for claim or timeout events",
                });
            } else {
                info["error"] = json!("Invalid output hash format - must be 64 hex characters");
            }
        }

        // Add swap-type specific information
        match swap_type {
            "sha" | "sha_atomic_swap" => {
                info["sha_swap_guide"] = json!({
                    "step1": "Sender creates HTLC with secret hash",
                    "step2": "Sender shares output hash with recipient",
                    "step3": "Recipient verifies swap details",
                    "step4": "Sender reveals pre-image to enable claiming",
                    "step5": "Recipient claims swap using pre-image",
                    "timelock": "Sender can refund if recipient doesn't claim in time",
                });
            },
            "htlc" | "htlc_refund" => {
                info["htlc_refund_guide"] = json!({
                    "eligibility": "Only available after timelock expiry",
                    "process": "Original sender can claim refund",
                    "verification": "Check that recipient hasn't already claimed",
                    "timing": "Act promptly once timelock expires",
                });
            },
            _ => {},
        }

        Ok(json!({
            "atomic_swap_info": info,
            "current_timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "resources": {
                "documentation": "Refer to Tari documentation for detailed atomic swap guides",
                "support": "Contact wallet support for assistance with specific swap issues",
                "tools": [
                    "send_sha_atomic_swap - Initiate new atomic swaps",
                    "claim_sha_atomic_swap - Claim received swaps",
                    "claim_htlc_refund - Recover expired swap funds"
                ]
            }
        }))
    }
}
