//! Simple transfer tool for testing

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{
    get_optional_string_param,
    get_required_number_param,
    get_required_string_param,
    McpError,
    McpResult,
    McpTool,
    PermissionLevel,
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::Value;
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
        "Send Tari cryptocurrency to another wallet address. This tool transfers funds from your wallet to any valid \
         Tari address. Before using this tool, ensure the wallet is ready (use check_wallet_state tool) and verify you \
         have sufficient balance. The transfer is irreversible once broadcast to the network. Requires wallet to be \
         unlocked and synced."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        minotari_mcp_common::json_schema! {
            "recipient" => serde_json::json!({
                "type": "string",
                "description": "Destination Tari wallet address (base58 encoded public key). Must be a valid Tari address format. Example: '9f8c3d4a5b6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b'",
                "pattern": "^[123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]+$",
                "minLength": 32
            }),
            "amount" => serde_json::json!({
                "type": "number",
                "description": "Amount to send in microTari (µT). 1 Tari = 1,000,000 microTari. Minimum: 1 µT. Must be a positive integer. Examples: 1000000 µT = 1 Tari, 500000 µT = 0.5 Tari",
                "minimum": 1,
                "multipleOf": 1
            }),
            "message" => serde_json::json!({
                "type": "string",
                "description": "Optional message to include with the transaction. Will be visible on the blockchain. Maximum 280 characters.",
                "maxLength": 280,
                "default": ""
            }),
            "fee_per_gram" => serde_json::json!({
                "type": "number",
                "description": "Transaction fee per gram in microTari. Higher fees result in faster processing. If not specified, wallet will calculate an appropriate fee automatically. Typical range: 1-100 µT/gram",
                "minimum": 1,
                "default": null
            })
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let recipient = get_required_string_param(params, "recipient")?;
        let amount = get_required_number_param(params, "amount")? as u64;

        // Validate recipient address format
        if recipient.is_empty() {
            return Err(McpError::invalid_request("Recipient address cannot be empty"));
        }

        if recipient.len() < 32 {
            return Err(McpError::invalid_request(
                "Recipient address too short - must be at least 32 characters",
            ));
        }

        // Basic base58 character validation
        if !recipient
            .chars()
            .all(|c| "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c))
        {
            return Err(McpError::invalid_request(
                "Recipient address contains invalid characters - must be valid base58",
            ));
        }

        // Validate amount
        if amount == 0 {
            return Err(McpError::invalid_request("Amount must be greater than 0 microTari"));
        }

        if amount > 21_000_000_000_000u64 {
            // 21M Tari in microTari
            return Err(McpError::invalid_request("Amount exceeds maximum possible Tari supply"));
        }

        // Validate optional message
        if let Some(message) = get_optional_string_param(params, "message") {
            if message.len() > 280 {
                return Err(McpError::invalid_request("Message cannot exceed 280 characters"));
            }
        }

        // Validate optional fee
        if let Some(fee) = params.get("fee_per_gram").and_then(|v| v.as_f64()) {
            if fee < 1.0 {
                return Err(McpError::invalid_request("Fee per gram must be at least 1 microTari"));
            }
            if fee > 10000.0 {
                return Err(McpError::invalid_request(
                    "Fee per gram too high - maximum 10000 microTari",
                ));
            }
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let recipient = get_required_string_param(&params, "recipient")?;
        let amount = get_required_number_param(&params, "amount")? as u64;
        let message = get_optional_string_param(&params, "message").unwrap_or_default();
        let fee_per_gram = params.get("fee_per_gram").and_then(|v| v.as_f64()).map(|f| f as u64);

        // For now, return a detailed placeholder response that shows what would happen
        // In a real implementation, this would call the wallet gRPC service
        let estimated_fee = fee_per_gram.unwrap_or(25); // Default 25 µT/gram
        let total_cost = amount + estimated_fee;

        Ok(serde_json::json!({
            "status": "simulated",
            "message": "Transfer prepared successfully (simulation mode - actual wallet integration pending)",
            "transaction_details": {
                "recipient": recipient,
                "amount_microtari": amount,
                "amount_tari": format!("{:.6}", amount as f64 / 1_000_000.0),
                "message": message,
                "estimated_fee_microtari": estimated_fee,
                "estimated_fee_tari": format!("{:.6}", estimated_fee as f64 / 1_000_000.0),
                "total_cost_microtari": total_cost,
                "total_cost_tari": format!("{:.6}", total_cost as f64 / 1_000_000.0)
            },
            "next_steps": [
                "This is currently a simulation - no actual funds were transferred",
                "To enable real transfers, the wallet gRPC integration must be completed",
                "Once enabled, this transaction would be broadcast to the Tari network",
                "Transaction typically confirms within 2-5 minutes"
            ],
            "security_note": "Always verify the recipient address before sending. Transactions cannot be reversed once confirmed on the blockchain."
        }))
    }
}
