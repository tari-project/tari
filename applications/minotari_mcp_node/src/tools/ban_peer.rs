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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol) infrastructure for Tari applications
//! Peer management MCP tools

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param, get_required_number_param, impl_mcp_tool, tool_schema
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{BanPeerRequest, UnbanPeerRequest}};
use async_trait::async_trait;
use serde_json::Value;

use tonic::transport::Channel;

/// Tool for banning peers
pub struct BanPeerTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl BanPeerTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

impl_mcp_tool!(BanPeerTool, privileged, {
    "type": "object",
    "properties": {
        "peer_public_key": {
            "type": "string",
            "description": "Public key of the peer to ban (hex encoded)"
        },
        "duration_hours": {
            "type": "number",
            "description": "Duration of the ban in hours (0 for permanent)",
            "minimum": 0
        },
        "reason": {
            "type": "string",
            "description": "Reason for banning the peer"
        }
    },
    "required": ["peer_public_key", "duration_hours", "reason"]
});

#[async_trait]
impl McpTool for BanPeerTool {
    fn name(&self) -> &str {
        "ban_peer"
    }

    fn description(&self) -> &str {
        "Ban a peer from connecting to this node. This is a control operation that affects network connectivity."
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let peer_key = get_required_string_param(params, "peer_public_key")?;
        let duration = get_required_number_param(params, "duration_hours")?;
        let reason = get_required_string_param(params, "reason")?;
        
        if peer_key.is_empty() {
            return Err(McpError::invalid_request("Peer public key cannot be empty"));
        }

        if !peer_key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Peer public key must be hex encoded"));
        }

        if duration < 0.0 {
            return Err(McpError::invalid_request("Duration cannot be negative"));
        }

        if reason.is_empty() {
            return Err(McpError::invalid_request("Reason cannot be empty"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let peer_key = get_required_string_param(&params, "peer_public_key")?;
        let duration_hours = get_required_number_param(&params, "duration_hours")?;
        let reason = get_required_string_param(&params, "reason")?;
        
        // Convert duration to seconds (0 for permanent)
        let duration_secs = if duration_hours == 0.0 {
            0
        } else {
            (duration_hours * 3600.0) as u64
        };

        let request = BanPeerRequest {
            peer_public_key: peer_key.clone(),
            duration: duration_secs,
            reason: reason.clone(),
        };

        // Submit ban request
        let mut client = self.grpc_client.clone().as_ref().clone();
        let response = client
            .ban_peer(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to ban peer: {}", e)))?;

        let _response = response.into_inner();

        // Return result
        Ok(serde_json::json!({
            "success": true,
            "peer_public_key": peer_key,
            "duration_hours": duration_hours,
            "reason": reason,
            "message": format!("Peer {} banned successfully", peer_key)
        }))
    }
}

/// Tool for unbanning peers
pub struct UnbanPeerTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl UnbanPeerTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

impl_mcp_tool!(UnbanPeerTool, privileged, {
    "type": "object",
    "properties": {
        "peer_public_key": {
            "type": "string",
            "description": "Public key of the peer to unban (hex encoded)"
        }
    },
    "required": ["peer_public_key"]
});

#[async_trait]
impl McpTool for UnbanPeerTool {
    fn name(&self) -> &str {
        "unban_peer"
    }

    fn description(&self) -> &str {
        "Remove a ban from a peer, allowing them to connect again. This is a control operation."
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let peer_key = get_required_string_param(params, "peer_public_key")?;
        
        if peer_key.is_empty() {
            return Err(McpError::invalid_request("Peer public key cannot be empty"));
        }

        if !peer_key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Peer public key must be hex encoded"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let peer_key = get_required_string_param(&params, "peer_public_key")?;
        
        let request = UnbanPeerRequest {
            peer_public_key: peer_key.clone(),
        };

        // Submit unban request
        let mut client = self.grpc_client.clone().as_ref().clone();
        let response = client
            .unban_peer(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to unban peer: {}", e)))?;

        let _response = response.into_inner();

        // Return result
        Ok(serde_json::json!({
            "success": true,
            "peer_public_key": peer_key,
            "message": format!("Peer {} unbanned successfully", peer_key)
        }))
    }
}
