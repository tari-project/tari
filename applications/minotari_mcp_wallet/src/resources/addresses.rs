//! Wallet addresses resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetAddressRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing wallet address information
pub struct AddressesResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl AddressesResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for AddressesResource {
    fn uri(&self) -> &str {
        "addresses"
    }

    fn name(&self) -> &str {
        "Wallet Addresses"
    }

    fn description(&self) -> &str {
        "Wallet addresses including interactive and one-sided payment addresses"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        // Get the default address
        let response = client
            .get_address(GetAddressRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get address: {}", e)))?;

        let address_info = response.into_inner();

        Ok(serde_json::json!({
            "default_address": {
                "address": address_info.address,
                "emoji_id": address_info.emoji_id,
                "public_key": address_info.public_key,
                "type": "interactive"
            },
            "address_types": {
                "interactive": {
                    "description": "For receiving payments with sender interaction",
                    "address": address_info.address,
                    "emoji_id": address_info.emoji_id
                },
                "one_sided": {
                    "description": "For receiving payments without sender interaction", 
                    "public_key": address_info.public_key,
                    "note": "One-sided payments use the public key directly"
                }
            },
            "usage_notes": {
                "interactive_payments": "Use address or emoji_id for normal transactions",
                "one_sided_payments": "Use public_key for stealth payments",
                "sharing": "Either address or emoji_id can be shared for receiving funds"
            }
        }))
    }
}
