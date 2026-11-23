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

use minotari_mcp_common::{
    McpResult,
    McpTool,
    PermissionLevel,
    wallet_rpc::{self, SpendLimits},
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{Value, json};
use tonic::transport::Channel;

/// Tool for sending SHA atomic swap transactions
#[derive(Clone)]
pub struct SendShaAtomicSwapTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
    limits: Arc<SpendLimits>,
}

impl SendShaAtomicSwapTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>, limits: Arc<SpendLimits>) -> Self {
        Self { grpc_client, limits }
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
        wallet_rpc::send_sha_atomic_swap((*self.grpc_client).clone(), &params, &self.limits).await
    }
}

/// Tool for claiming SHA atomic swap transactions
#[derive(Clone)]
pub struct ClaimShaAtomicSwapTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
    limits: Arc<SpendLimits>,
}

impl ClaimShaAtomicSwapTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>, limits: Arc<SpendLimits>) -> Self {
        Self { grpc_client, limits }
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
        wallet_rpc::claim_sha_atomic_swap((*self.grpc_client).clone(), &params, &self.limits).await
    }
}

/// Tool for claiming HTLC refunds
#[derive(Clone)]
pub struct ClaimHtlcRefundTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
    limits: Arc<SpendLimits>,
}

impl ClaimHtlcRefundTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>, limits: Arc<SpendLimits>) -> Self {
        Self { grpc_client, limits }
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
        wallet_rpc::claim_htlc_refund((*self.grpc_client).clone(), &params, &self.limits).await
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

#[allow(clippy::indexing_slicing)]
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
