//! Simple transfer tool for testing

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    get_required_string_param, get_required_number_param
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Simple tool for transferring Tari - basic implementation
pub struct SimpleTransferTool {
    #[allow(dead_code)]
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl SimpleTransferTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for SimpleTransferTool {
    fn name(&self) -> &str {
        "simple_transfer"
    }

    fn description(&self) -> &str {
        "Simple transfer tool for sending Tari (placeholder implementation)"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        minotari_mcp_common::json_schema! {
            "recipient" => serde_json::json!({
                "type": "string", 
                "description": "Destination address to send Tari to"
            }),
            "amount" => serde_json::json!({
                "type": "number",
                "description": "Amount of microTari to send" 
            })
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let recipient = get_required_string_param(params, "recipient")?;
        let amount = get_required_number_param(params, "amount")? as u64;
        
        if recipient.is_empty() {
            return Err(McpError::invalid_request("Recipient cannot be empty"));
        }

        if amount == 0 {
            return Err(McpError::invalid_request("Amount must be greater than 0"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let recipient = get_required_string_param(&params, "recipient")?;
        let amount = get_required_number_param(&params, "amount")? as u64;

        // For now, just return a placeholder response
        // In a real implementation, this would call the wallet gRPC service
        Ok(serde_json::json!({
            "success": true,
            "message": "Transfer functionality not yet implemented",
            "recipient": recipient,
            "amount": amount,
            "note": "This is a placeholder - actual wallet integration pending"
        }))
    }
}
