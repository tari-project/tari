//! Peer management MCP tools

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param, get_required_number_param
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{BanPeerRequest, UnbanPeerRequest}};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for banning peers
pub struct BanPeerTool {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl BanPeerTool {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for BanPeerTool {
    fn name(&self) -> &str {
        "ban_peer"
    }

    fn description(&self) -> &str {
        "Ban a peer from connecting to this node. This is a control operation that affects network connectivity."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "peer_public_key" => {
                "type": "string",
                "description": "Public key of the peer to ban (hex encoded)"
            },
            "duration_hours" => {
                "type": "number",
                "description": "Duration of the ban in hours (0 for permanent)",
                "minimum": 0
            },
            "reason" => {
                "type": "string",
                "description": "Reason for banning the peer"
            }
        }
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
        let mut client = self.grpc_client.as_ref().clone();
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
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl UnbanPeerTool {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for UnbanPeerTool {
    fn name(&self) -> &str {
        "unban_peer"
    }

    fn description(&self) -> &str {
        "Remove a ban from a peer, allowing them to connect again. This is a control operation."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "peer_public_key" => {
                "type": "string",
                "description": "Public key of the peer to unban (hex encoded)"
            }
        }
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
        let mut client = self.grpc_client.as_ref().clone();
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
