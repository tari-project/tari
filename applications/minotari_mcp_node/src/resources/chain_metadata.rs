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
//! Chain metadata resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing current blockchain metadata
pub struct ChainMetadataResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl ChainMetadataResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for ChainMetadataResource {
    fn uri(&self) -> &str {
        "chain_metadata"
    }

    fn name(&self) -> &str {
        "Chain Metadata"
    }

    fn description(&self) -> &str {
        "Current blockchain metadata including height, best block hash, and chain tips"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();

        let response = client
            .get_tip_info(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get chain metadata: {e}")))?;

        let tip_info = response.into_inner();

        if let Some(metadata) = tip_info.metadata {
            Ok(serde_json::json!({
                "best_block_height": metadata.best_block_height,
                "best_block_hash": hex::encode(&metadata.best_block_hash),
                "accumulated_difficulty": hex::encode(&metadata.accumulated_difficulty),
                "pruned_height": metadata.pruned_height,
                "timestamp": metadata.timestamp,
                "chain_metadata": {
                    "best_block_height": metadata.best_block_height,
                    "accumulated_difficulty": hex::encode(&metadata.accumulated_difficulty),
                    "pruned_height": metadata.pruned_height,
                }
            }))
        } else {
            Ok(serde_json::json!({
                "error": "No metadata available",
                "initial_sync_achieved": tip_info.initial_sync_achieved,
                "base_node_state": tip_info.base_node_state
            }))
        }
    }
}
