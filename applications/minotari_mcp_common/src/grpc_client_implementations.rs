//! Concrete gRPC Client Implementations
//!
//! This module provides concrete implementations of the NodeGrpcClient and WalletGrpcClient
//! traits that wrap the actual Tari gRPC clients and handle parameter conversion.

use crate::{
    grpc_executor::{NodeGrpcClient, WalletGrpcClient},
    McpResult, McpError,
};
use serde_json::{Value, json};
use std::sync::Arc;
use async_trait::async_trait;
use tonic::transport::Channel;

/// Node gRPC client implementation wrapping BaseNodeGrpcClient
pub struct NodeGrpcClientImpl {
    /// The actual base node gRPC client
    client: Arc<minotari_node_grpc_client::BaseNodeGrpcClient<Channel>>,
}

impl NodeGrpcClientImpl {
    /// Create a new node client implementation
    pub fn new(client: Arc<minotari_node_grpc_client::BaseNodeGrpcClient<Channel>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl NodeGrpcClient for NodeGrpcClientImpl {
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value> {
        // This is a fallback for methods not explicitly implemented
        // For now, return an error indicating the method needs explicit implementation
        Err(McpError::server_error(format!(
            "Method '{}' not explicitly implemented in NodeGrpcClientImpl. Parameters: {}",
            method_name, parameters
        )))
    }

    async fn get_tip_info(&self) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::Empty;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(Empty {});
        
        match client.get_tip_info(request).await {
            Ok(response) => {
                let tip_info = response.into_inner();
                Ok(json!({
                    "height": tip_info.metadata.map(|m| m.height_of_longest_chain).unwrap_or(0),
                    "best_block_hash": tip_info.metadata.map(|m| hex::encode(m.best_block_hash)).unwrap_or_default(),
                    "accumulated_difficulty": tip_info.metadata.map(|m| hex::encode(m.accumulated_difficulty)).unwrap_or_default(),
                    "pruned_height": tip_info.metadata.map(|m| m.pruned_height).unwrap_or(0),
                    "timestamp": tip_info.metadata.map(|m| m.timestamp).unwrap_or(0),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get tip info: {}", e))),
        }
    }

    async fn get_network_status(&self) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::Empty;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(Empty {});
        
        match client.get_network_status(request).await {
            Ok(response) => {
                let status = response.into_inner();
                Ok(json!({
                    "status": status.status,
                    "avg_latency": status.avg_latency_ms,
                    "num_node_connections": status.num_node_connections,
                    "connection_status": status.connection_status,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get network status: {}", e))),
        }
    }

    async fn get_peers(&self) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::GetPeersRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetPeersRequest {});
        
        match client.get_peers(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let mut peers = Vec::new();
                
                while let Some(peer_response) = stream.message().await.map_err(|e| {
                    McpError::server_error(format!("Error reading peers stream: {}", e))
                })? {
                    if let Some(peer) = peer_response.peer {
                        peers.push(json!({
                            "public_key": hex::encode(peer.public_key),
                            "addresses": peer.addresses,
                            "flags": peer.flags,
                            "banned_until": peer.banned_until,
                            "banned_reason": peer.banned_reason,
                            "offline_at": peer.offline_at,
                            "last_seen": peer.last_seen,
                        }));
                    }
                }
                
                Ok(json!({
                    "peers": peers,
                    "total_count": peers.len(),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get peers: {}", e))),
        }
    }

    async fn get_new_block_template(&self, algorithm: Option<String>) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::{NewBlockTemplateRequest, PowAlgorithm};
        
        let pow_algo = match algorithm.as_deref() {
            Some("sha3x") => PowAlgorithm::Sha3x,
            Some("randomxm") => PowAlgorithm::RandomX,
            Some("randomxt") => PowAlgorithm::RandomX, // Assume RandomX for both variants
            _ => PowAlgorithm::Sha3x, // Default
        };
        
        let mut client = self.client.client();
        let request = tonic::Request::new(NewBlockTemplateRequest {
            algo: Some(pow_algo.into()),
            max_weight: 0, // Use default
        });
        
        match client.get_new_block_template(request).await {
            Ok(response) => {
                let template = response.into_inner();
                Ok(json!({
                    "miner_data": template.miner_data.map(|data| json!({
                        "algorithm": algorithm.unwrap_or_else(|| "sha3x".to_string()),
                        "target_difficulty": data.target_difficulty,
                        "reward": data.reward,
                        "total_fees": data.total_fees,
                        "pow_data": hex::encode(data.pow_data),
                    })),
                    "new_block_template": template.new_block_template.map(|block| json!({
                        "header": block.header.map(|h| hex::encode(h.hash)),
                        "body": block.body.map(|b| json!({
                            "inputs": b.inputs.len(),
                            "outputs": b.outputs.len(),
                            "kernels": b.kernels.len(),
                        })),
                    })),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get new block template: {}", e))),
        }
    }

    async fn submit_block(&self, block_data: &str) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::Block;
        
        // Decode hex block data
        let block_bytes = hex::decode(block_data)
            .map_err(|e| McpError::invalid_params(format!("Invalid hex block data: {}", e)))?;
        
        // For now, we need to properly deserialize the block from bytes
        // This is a simplified implementation - in practice, you'd need proper protobuf deserialization
        let block = Block {
            header: None, // Would need proper deserialization
            body: None,   // Would need proper deserialization
        };
        
        let mut client = self.client.client();
        let request = tonic::Request::new(block);
        
        match client.submit_block(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "block_hash": hex::encode(result.block_hash),
                    "success": true,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to submit block: {}", e))),
        }
    }

    async fn submit_transaction(&self, transaction_data: &str) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::SubmitTransactionRequest;
        
        // Decode hex transaction data
        let transaction_bytes = hex::decode(transaction_data)
            .map_err(|e| McpError::invalid_params(format!("Invalid hex transaction data: {}", e)))?;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(SubmitTransactionRequest {
            transaction_bytes,
        });
        
        match client.submit_transaction(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "transaction_id": hex::encode(result.transaction_id),
                    "success": true,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to submit transaction: {}", e))),
        }
    }

    async fn get_mempool_stats(&self) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::Empty;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(Empty {});
        
        match client.get_mempool_stats(request).await {
            Ok(response) => {
                let stats = response.into_inner();
                Ok(json!({
                    "unconfirmed_txs": stats.unconfirmed_txs,
                    "reorg_txs": stats.reorg_txs,
                    "unconfirmed_weight": stats.unconfirmed_weight,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get mempool stats: {}", e))),
        }
    }

    async fn get_sync_info(&self) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::Empty;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(Empty {});
        
        match client.get_sync_info(request).await {
            Ok(response) => {
                let sync_info = response.into_inner();
                Ok(json!({
                    "tip_height": sync_info.tip_height,
                    "local_height": sync_info.local_height,
                    "peer_info": sync_info.peer_info.into_iter().map(|peer| json!({
                        "node_id": hex::encode(peer.node_id),
                        "chain_metadata": peer.chain_metadata.map(|meta| json!({
                            "height_of_longest_chain": meta.height_of_longest_chain,
                            "best_block_hash": hex::encode(meta.best_block_hash),
                        })),
                    })).collect::<Vec<_>>(),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get sync info: {}", e))),
        }
    }

    async fn list_headers(&self, from_height: Option<u64>, to_height: Option<u64>) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::ListHeadersRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(ListHeadersRequest {
            from_height: from_height.unwrap_or(0),
            num_headers: to_height.map(|to| to - from_height.unwrap_or(0) + 1).unwrap_or(1),
            sorting: 0, // Default sorting
        });
        
        match client.list_headers(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let mut headers = Vec::new();
                
                while let Some(header_response) = stream.message().await.map_err(|e| {
                    McpError::server_error(format!("Error reading headers stream: {}", e))
                })? {
                    if let Some(header) = header_response.header {
                        headers.push(json!({
                            "hash": hex::encode(header.hash),
                            "version": header.version,
                            "height": header.height,
                            "prev_hash": hex::encode(header.prev_hash),
                            "timestamp": header.timestamp,
                            "merkle_root": hex::encode(header.merkle_root),
                        }));
                    }
                }
                
                Ok(json!({
                    "headers": headers,
                    "count": headers.len(),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to list headers: {}", e))),
        }
    }

    async fn get_blocks(&self, from_height: u64, to_height: Option<u64>) -> McpResult<Value> {
        use minotari_node_grpc_client::grpc::GetBlocksRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetBlocksRequest {
            heights: vec![from_height], // Simplified - would normally handle range
        });
        
        match client.get_blocks(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let mut blocks = Vec::new();
                
                while let Some(block_response) = stream.message().await.map_err(|e| {
                    McpError::server_error(format!("Error reading blocks stream: {}", e))
                })? {
                    if let Some(block) = block_response.block {
                        blocks.push(json!({
                            "header": block.header.map(|h| json!({
                                "hash": hex::encode(h.hash),
                                "height": h.height,
                                "timestamp": h.timestamp,
                            })),
                            "body": block.body.map(|b| json!({
                                "inputs": b.inputs.len(),
                                "outputs": b.outputs.len(),
                                "kernels": b.kernels.len(),
                            })),
                        }));
                    }
                }
                
                Ok(json!({
                    "blocks": blocks,
                    "count": blocks.len(),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get blocks: {}", e))),
        }
    }
}

/// Wallet gRPC client implementation wrapping WalletGrpcClient
pub struct WalletGrpcClientImpl {
    /// The actual wallet gRPC client
    client: Arc<minotari_wallet_grpc_client::WalletGrpcClient<Channel>>,
}

impl WalletGrpcClientImpl {
    /// Create a new wallet client implementation
    pub fn new(client: Arc<minotari_wallet_grpc_client::WalletGrpcClient<Channel>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WalletGrpcClient for WalletGrpcClientImpl {
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value> {
        // This is a fallback for methods not explicitly implemented
        Err(McpError::server_error(format!(
            "Method '{}' not explicitly implemented in WalletGrpcClientImpl. Parameters: {}",
            method_name, parameters
        )))
    }

    async fn get_balance(&self) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::GetBalanceRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetBalanceRequest {});
        
        match client.get_balance(request).await {
            Ok(response) => {
                let balance = response.into_inner();
                Ok(json!({
                    "available_balance": balance.available_balance,
                    "time_locked_balance": balance.time_locked_balance,
                    "pending_incoming_balance": balance.pending_incoming_balance,
                    "pending_outgoing_balance": balance.pending_outgoing_balance,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get balance: {}", e))),
        }
    }

    async fn transfer(&self, recipient: &str, amount: u64, fee_per_gram: Option<u64>, message: Option<&str>) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::TransferRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(TransferRequest {
            recipients: vec![minotari_wallet_grpc_client::grpc::PaymentRecipient {
                address: recipient.to_string(),
                amount,
                fee_per_gram: fee_per_gram.unwrap_or(25),
                message: message.unwrap_or("").to_string(),
                payment_type: 0, // Standard payment
            }],
        });
        
        match client.transfer(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "transaction_id": result.transaction_id,
                    "is_success": result.is_success,
                    "failure_message": result.failure_message,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to transfer: {}", e))),
        }
    }

    async fn get_transaction_info(&self, transaction_id: &str) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::GetTransactionInfoRequest;
        
        // Parse transaction ID (could be numeric or hex)
        let tx_id = if let Ok(id) = transaction_id.parse::<u64>() {
            id
        } else {
            return Err(McpError::invalid_params("Invalid transaction ID format"));
        };
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        });
        
        match client.get_transaction_info(request).await {
            Ok(response) => {
                let transactions = response.into_inner();
                if let Some(tx) = transactions.transactions.first() {
                    Ok(json!({
                        "transaction_id": tx.transaction_id,
                        "source_address": tx.source_address,
                        "dest_address": tx.dest_address,
                        "status": tx.status,
                        "amount": tx.amount,
                        "fee": tx.fee,
                        "is_cancelled": tx.is_cancelled,
                        "excess_sig": tx.excess_sig,
                        "timestamp": tx.timestamp,
                        "message": tx.message,
                    }))
                } else {
                    Err(McpError::server_error("Transaction not found"))
                }
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get transaction info: {}", e))),
        }
    }

    async fn cancel_transaction(&self, transaction_id: &str) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::CancelTransactionRequest;
        
        let tx_id = if let Ok(id) = transaction_id.parse::<u64>() {
            id
        } else {
            return Err(McpError::invalid_params("Invalid transaction ID format"));
        };
        
        let mut client = self.client.client();
        let request = tonic::Request::new(CancelTransactionRequest {
            transaction_id: tx_id,
        });
        
        match client.cancel_transaction(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "is_success": result.is_success,
                    "failure_message": result.failure_message,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to cancel transaction: {}", e))),
        }
    }

    async fn create_burn_transaction(&self, amount: u64, fee_per_gram: Option<u64>, message: Option<&str>, claim_public_key: Option<&str>) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::CreateBurnTransactionRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(CreateBurnTransactionRequest {
            amount,
            fee_per_gram: fee_per_gram.unwrap_or(25),
            message: message.unwrap_or("").to_string(),
            claim_public_key: claim_public_key.map(|key| {
                hex::decode(key).unwrap_or_default()
            }).unwrap_or_default(),
        });
        
        match client.create_burn_transaction(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "transaction_id": result.transaction_id,
                    "is_success": result.is_success,
                    "failure_message": result.failure_message,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to create burn transaction: {}", e))),
        }
    }

    async fn coin_split(&self, amount: u64, count: u64, fee_per_gram: Option<u64>, message: Option<&str>) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::CoinSplitRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(CoinSplitRequest {
            amount,
            split_count: count,
            fee_per_gram: fee_per_gram.unwrap_or(25),
            message: message.unwrap_or("").to_string(),
        });
        
        match client.coin_split(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "transaction_id": result.transaction_id,
                    "is_success": result.is_success,
                    "failure_message": result.failure_message,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to split coins: {}", e))),
        }
    }

    async fn import_utxos(&self, outputs: Vec<String>, source_public_keys: Vec<String>) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::ImportUtxosRequest;
        
        // Convert hex strings to bytes
        let output_bytes: Result<Vec<Vec<u8>>, _> = outputs.iter()
            .map(|hex| hex::decode(hex))
            .collect();
        let output_bytes = output_bytes
            .map_err(|e| McpError::invalid_params(format!("Invalid hex output: {}", e)))?;
        
        let key_bytes: Result<Vec<Vec<u8>>, _> = source_public_keys.iter()
            .map(|hex| hex::decode(hex))
            .collect();
        let key_bytes = key_bytes
            .map_err(|e| McpError::invalid_params(format!("Invalid hex public key: {}", e)))?;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(ImportUtxosRequest {
            outputs: output_bytes,
            source_public_keys: key_bytes,
        });
        
        match client.import_utxos(request).await {
            Ok(response) => {
                let result = response.into_inner();
                Ok(json!({
                    "num_imported": result.num_imported,
                    "is_success": result.num_imported > 0,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to import UTXOs: {}", e))),
        }
    }

    async fn get_addresses(&self) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::GetAddressRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetAddressRequest {});
        
        match client.get_address(request).await {
            Ok(response) => {
                let address_info = response.into_inner();
                Ok(json!({
                    "address": address_info.address,
                    "emoji_id": address_info.emoji_id,
                    "public_key": hex::encode(address_info.public_key),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get addresses: {}", e))),
        }
    }

    async fn get_connected_peers(&self) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::GetConnectedPeersRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetConnectedPeersRequest {});
        
        match client.get_connected_peers(request).await {
            Ok(response) => {
                let peers_info = response.into_inner();
                let peers: Vec<Value> = peers_info.connected_peers.into_iter().map(|peer| {
                    json!({
                        "address": peer.address,
                        "node_id": hex::encode(peer.node_id),
                        "direction": peer.direction,
                        "age": peer.age,
                        "latency": peer.latency,
                        "user_agent": peer.user_agent,
                    })
                }).collect();
                
                Ok(json!({
                    "connected_peers": peers,
                    "count": peers.len(),
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get connected peers: {}", e))),
        }
    }

    async fn get_network_status(&self) -> McpResult<Value> {
        use minotari_wallet_grpc_client::grpc::GetNetworkStatusRequest;
        
        let mut client = self.client.client();
        let request = tonic::Request::new(GetNetworkStatusRequest {});
        
        match client.get_network_status(request).await {
            Ok(response) => {
                let status = response.into_inner();
                Ok(json!({
                    "status": status.status,
                    "avg_latency": status.avg_latency_ms,
                    "num_node_connections": status.num_node_connections,
                }))
            }
            Err(e) => Err(McpError::server_error(format!("Failed to get network status: {}", e))),
        }
    }
}
