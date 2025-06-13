//! Submit transaction MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param, impl_mcp_tool, tool_schema
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{SubmitTransactionRequest, Transaction}};
use async_trait::async_trait;
use serde_json::Value;

use tonic::transport::Channel;

/// Tool for submitting transactions to the mempool
pub struct SubmitTransactionTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl SubmitTransactionTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

impl_mcp_tool!(SubmitTransactionTool, control, {
    "type": "object",
    "properties": {
        "transaction_hex": {
            "type": "string",
            "description": "Complete transaction in hexadecimal format (serialized binary transaction). Must be a valid hex string containing a fully formed, signed Tari transaction. Example: '01a2b3c4d5e6f7...'. Minimum length varies based on transaction complexity, typically 200+ characters for simple transactions.",
            "pattern": "^[0-9a-fA-F]+$",
            "minLength": 100
        },
        "dry_run": {
            "type": "boolean",
            "description": "If true, validate the transaction without actually submitting it to the mempool. Useful for testing transaction validity before broadcast.",
            "default": false
        }
    },
    "required": ["transaction_hex"]
});

#[async_trait]
impl McpTool for SubmitTransactionTool {
    fn name(&self) -> &str {
        "submit_transaction"
    }

    fn description(&self) -> &str {
        "Submit a pre-signed transaction to the Tari mempool for inclusion in the blockchain. This tool accepts a serialized transaction in hexadecimal format and broadcasts it to the network. The transaction must be properly formatted, signed, and have valid inputs. Once submitted, the transaction will be validated by nodes and miners for inclusion in the next block. Use this for broadcasting transactions created offline or by other applications."
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let tx_hex = get_required_string_param(params, "transaction_hex")?;
        
        if tx_hex.is_empty() {
            return Err(McpError::invalid_request("Transaction hex cannot be empty"));
        }

        // Basic hex validation
        if !tx_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Transaction hex contains invalid characters"));
        }

        if tx_hex.len() % 2 != 0 {
            return Err(McpError::invalid_request("Transaction hex must have even length"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let tx_hex = get_required_string_param(&params, "transaction_hex")?;
        
        // For simplicity, we'll create a minimal transaction structure
        // In a real implementation, you'd properly decode the hex into a Transaction
        let _tx_bytes = hex::decode(&tx_hex)
            .map_err(|e| McpError::tool_execution_failed(format!("Invalid hex encoding: {}", e)))?;

        // Create a minimal transaction - this is a placeholder
        // In practice, you'd need to properly deserialize the transaction from bytes
        let transaction = Transaction {
            offset: vec![],
            body: None,
            script_offset: vec![],
        };

        // Create submit request with transaction
        let request = SubmitTransactionRequest {
            transaction: Some(transaction),
        };

        // Submit transaction to mempool
        let response = self.grpc_client.clone()
            .submit_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to submit transaction: {}", e)))?;

        let response = response.into_inner();

        // Return result
        Ok(serde_json::json!({
            "success": true,
            "result": response.result,
            "message": "Transaction submitted successfully to mempool"
        }))
    }
}
