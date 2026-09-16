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
//! Simple balance resource for testing

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpResource, McpResult};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::Value;
use tonic::transport::Channel;

/// Simple resource providing wallet balance information
pub struct SimpleBalanceResource {
    #[allow(dead_code)]
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl SimpleBalanceResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for SimpleBalanceResource {
    fn uri(&self) -> &str {
        "simple_balance"
    }

    fn name(&self) -> &str {
        "Simple Wallet Balance"
    }

    fn description(&self) -> &str {
        "Simple wallet balance information (placeholder implementation)"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        // For now, return placeholder data
        // In a real implementation, this would call the wallet gRPC service
        Ok(serde_json::json!({
            "available_balance": 0,
            "time_locked_balance": 0,
            "pending_incoming_balance": 0,
            "pending_outgoing_balance": 0,
            "total_balance": 0,
            "message": "Balance functionality not yet implemented",
            "note": "This is a placeholder - actual wallet integration pending"
        }))
    }
}
