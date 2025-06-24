// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! GRPC-based blockchain scanner implementation
//! 
//! This module provides a GRPC implementation of the BlockchainScanner trait
//! that connects to a Tari base node via GRPC to scan for wallet outputs.

#[cfg(feature = "grpc")]
use std::time::Duration;

#[cfg(feature = "grpc")]
use async_trait::async_trait;
#[cfg(feature = "grpc")]
use tonic::{transport::Channel, Request};
#[cfg(feature = "grpc")]
use tracing::{debug, error, warn};

#[cfg(feature = "grpc")]
use crate::{
    data_structures::{
        transaction_output::LightweightTransactionOutput,
        wallet_output::{
            LightweightWalletOutput, LightweightOutputFeatures, LightweightRangeProof,
            LightweightScript, LightweightSignature, LightweightCovenant
        },
        types::{CompressedCommitment, CompressedPublicKey, MicroMinotari},
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        LightweightOutputType,
        LightweightRangeProofType,
    },
    errors::{LightweightWalletError, LightweightWalletResult},
    extraction::{extract_wallet_output, ExtractionConfig},
    scanning::{BlockInfo, BlockScanResult, BlockchainScanner, ScanConfig, TipInfo, WalletScanner, WalletScanConfig, WalletScanResult, ProgressCallback, DefaultScanningLogic},
    key_management::{ConcreteKeyManager, KeyStore},
};

#[cfg(feature = "grpc")]
use crate::tari_rpc;

/// GRPC client for connecting to Tari base node
#[cfg(feature = "grpc")]
pub struct GrpcBlockchainScanner {
    /// GRPC channel to the base node
    client: tari_rpc::base_node_client::BaseNodeClient<Channel>,
    /// Connection timeout
    timeout: Duration,
    /// Base URL for the GRPC connection
    base_url: String,
}


#[cfg(feature = "grpc")]
impl GrpcBlockchainScanner {
    /// Create a new GRPC scanner with the given base URL
    pub async fn new(base_url: String) -> LightweightWalletResult<Self> {
        let timeout = Duration::from_secs(30);
        let channel = Channel::from_shared(base_url.clone())
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Invalid URL: {}", e))
            ))?
            .timeout(timeout)
            .connect()
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Connection failed: {}", e))
            ))?;

        let client = tari_rpc::base_node_client::BaseNodeClient::new(channel);

        Ok(Self {
            client,
            timeout,
            base_url,
        })
    }

    /// Create a new GRPC scanner with custom timeout
    pub async fn with_timeout(base_url: String, timeout: Duration) -> LightweightWalletResult<Self> {
        let channel = Channel::from_shared(base_url.clone())
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Invalid URL: {}", e))
            ))?
            .timeout(timeout)
            .connect()
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Connection failed: {}", e))
            ))?;

        let client = tari_rpc::base_node_client::BaseNodeClient::new(channel);

        Ok(Self {
            client,
            timeout,
            base_url,
        })
    }

    /// Convert GRPC transaction output to lightweight transaction output
    fn convert_transaction_output(
        grpc_output: &tari_rpc::TransactionOutput,
    ) -> LightweightTransactionOutput {
        // Convert OutputFeatures
        let features = grpc_output.features.as_ref().map(|f| {
            LightweightOutputFeatures {
                output_type: match f.output_type {
                    0 => LightweightOutputType::Payment,
                    1 => LightweightOutputType::Coinbase,
                    2 => LightweightOutputType::Burn,
                    3 => LightweightOutputType::ValidatorNodeRegistration,
                    4 => LightweightOutputType::CodeTemplateRegistration,
                    _ => LightweightOutputType::Payment,
                },
                maturity: f.maturity,
                range_proof_type: match f.range_proof_type {
                    0 => LightweightRangeProofType::BulletProofPlus,
                    1 => LightweightRangeProofType::RevealedValue,
                    _ => LightweightRangeProofType::BulletProofPlus,
                },
            }
        }).unwrap_or_default();

        // Convert Commitment - need to handle the 33-byte array properly
        let commitment_bytes = if grpc_output.commitment.len() == 33 {
            let mut bytes = [0u8; 33];
            bytes.copy_from_slice(&grpc_output.commitment);
            CompressedCommitment::new(bytes)
        } else {
            // Fallback to default if wrong size
            CompressedCommitment::new([0u8; 33])
        };

        // Convert RangeProof
        let proof = grpc_output.range_proof.as_ref().map(|rp| LightweightRangeProof { bytes: rp.proof_bytes.clone() });
        
        // Convert Script
        let script = LightweightScript { bytes: grpc_output.script.clone() };
        
        // Convert Sender Offset Public Key - need to handle the 32-byte array properly
        let sender_offset_public_key = if grpc_output.sender_offset_public_key.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&grpc_output.sender_offset_public_key);
            CompressedPublicKey::new(bytes)
        } else {
            // Fallback to default if wrong size
            CompressedPublicKey::new([0u8; 32])
        };
        
        // Convert Metadata Signature
        let metadata_signature = grpc_output.metadata_signature.as_ref().map(|sig| LightweightSignature { bytes: sig.u_a.clone() }).unwrap_or_default();
        
        // Convert Covenant
        let covenant = LightweightCovenant { bytes: grpc_output.covenant.clone() };
        
        // Convert Encrypted Data
        let encrypted_data = EncryptedData::from_bytes(&grpc_output.encrypted_data).unwrap_or_default();
        
        // Convert Minimum Value Promise
        let minimum_value_promise = MicroMinotari::new(grpc_output.minimum_value_promise);

        LightweightTransactionOutput {
            version: grpc_output.version as u8,
            features,
            commitment: commitment_bytes,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_data,
            minimum_value_promise,
        }
    }

    /// Convert GRPC block to lightweight block info
    fn convert_block(
        grpc_block: &tari_rpc::HistoricalBlock,
    ) -> Option<BlockInfo> {
        let block = grpc_block.block.as_ref()?;
        let header = block.header.as_ref()?;
        let body = block.body.as_ref()?;
        let outputs = body.outputs.iter().map(Self::convert_transaction_output).collect();
        Some(BlockInfo {
            height: header.height,
            hash: header.hash.clone(),
            timestamp: header.timestamp,
            outputs,
        })
    }

    /// Convert GRPC tip info to lightweight tip info
    fn convert_tip_info(grpc_tip: &tari_rpc::TipInfoResponse) -> TipInfo {
        let metadata = grpc_tip.metadata.as_ref();
        TipInfo {
            best_block_height: metadata.map(|m| m.best_block_height).unwrap_or(0),
            best_block_hash: metadata.map(|m| m.best_block_hash.clone()).unwrap_or_default(),
            accumulated_difficulty: metadata.map(|m| m.accumulated_difficulty.clone()).unwrap_or_default(),
            pruned_height: metadata.map(|m| m.pruned_height).unwrap_or(0),
            timestamp: metadata.map(|m| m.timestamp).unwrap_or(0),
        }
    }
}

#[cfg(feature = "grpc")]
#[async_trait]
impl BlockchainScanner for GrpcBlockchainScanner {
    async fn scan_blocks(
        &mut self,
        config: ScanConfig,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        debug!("Starting GRPC block scan from height {} to {:?}", config.start_height, config.end_height);
        
        // Get tip info to determine end height
        let tip_info = self.get_tip_info().await?;
        let end_height = config.end_height.unwrap_or(tip_info.best_block_height);

        if config.start_height > end_height {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut current_height = config.start_height;

        while current_height <= end_height {
            let batch_end = std::cmp::min(current_height + config.batch_size - 1, end_height);
            let heights: Vec<u64> = (current_height..=batch_end).collect();
            // Get blocks for this batch
            let request = tari_rpc::GetBlocksRequest {
                heights,
            };

            let mut stream = self.client
                .clone()
                .get_blocks(Request::new(request))
                .await
                .map_err(|e| LightweightWalletError::ScanningError(
                    crate::errors::ScanningError::blockchain_connection_failed(&format!("GRPC error: {}", e))
                ))?
                .into_inner();

            let mut batch_results = Vec::new();
            while let Some(grpc_block) = stream
                .message()
                .await
                .map_err(|e| LightweightWalletError::ScanningError(
                    crate::errors::ScanningError::blockchain_connection_failed(&format!("Stream error: {}", e))
                ))?
            {
                if let Some(block_info) = Self::convert_block(&grpc_block) {
                    let mut wallet_outputs = Vec::new();
                    // Extract wallet outputs from transaction outputs
                    for output in &block_info.outputs {
                        match extract_wallet_output(output, &config.extraction_config) {
                            Ok(wallet_output) => wallet_outputs.push(wallet_output),
                            Err(e) => {
                                debug!("Failed to extract wallet output: {}", e);
                            }
                        }
                    }
                    batch_results.push(BlockScanResult {
                        height: block_info.height,
                        block_hash: block_info.hash,
                        outputs: block_info.outputs,
                        wallet_outputs,
                        mined_timestamp: block_info.timestamp,
                    });
                }
            }

            results.extend(batch_results);
            current_height = batch_end + 1;
        }

        debug!("GRPC scan completed, found {} blocks with wallet outputs", results.len());
        Ok(results)
    }

    async fn get_tip_info(&mut self) -> LightweightWalletResult<TipInfo> {
        let request = Request::new(tari_rpc::Empty {});
        
        let response = self.client
            .clone()
            .get_tip_info(request)
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("GRPC error: {}", e))
            ))?;

        let tip_info = response.into_inner();
        Ok(Self::convert_tip_info(&tip_info))
    }

    async fn search_utxos(
        &mut self,
        commitments: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        let request = tari_rpc::SearchUtxosRequest {
            commitments,
        };

        let mut stream = self.client
            .clone()
            .search_utxos(Request::new(request))
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("GRPC error: {}", e))
            ))?
            .into_inner();

        let mut results = Vec::new();
        while let Some(grpc_block) = stream
            .message()
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Stream error: {}", e))
            ))?
        {
            if let Some(block_info) = Self::convert_block(&grpc_block) {
                let mut wallet_outputs = Vec::new();
                for output in &block_info.outputs {
                    match extract_wallet_output(output, &ExtractionConfig::default()) {
                        Ok(wallet_output) => wallet_outputs.push(wallet_output),
                        Err(e) => {
                            debug!("Failed to extract wallet output: {}", e);
                        }
                    }
                }
                results.push(BlockScanResult {
                    height: block_info.height,
                    block_hash: block_info.hash,
                    outputs: block_info.outputs,
                    wallet_outputs,
                    mined_timestamp: block_info.timestamp,
                });
            }
        }

        Ok(results)
    }

    async fn fetch_utxos(
        &mut self,
        hashes: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<LightweightTransactionOutput>> {
        let request = tari_rpc::FetchMatchingUtxosRequest {
            hashes,
        };

        let mut stream = self.client
            .clone()
            .fetch_matching_utxos(Request::new(request))
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("GRPC error: {}", e))
            ))?
            .into_inner();

        let mut results = Vec::new();
        while let Some(response) = stream
            .message()
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Stream error: {}", e))
            ))?
        {
            if let Some(output) = response.output {
                results.push(Self::convert_transaction_output(&output));
            }
        }

        Ok(results)
    }

    async fn get_blocks_by_heights(
        &mut self,
        heights: Vec<u64>,
    ) -> LightweightWalletResult<Vec<BlockInfo>> {
        if heights.is_empty() {
            return Ok(Vec::new());
        }

        let request = tari_rpc::GetBlocksRequest {
            heights,
        };

        let mut stream = self.client
            .clone()
            .get_blocks(Request::new(request))
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("GRPC error: {}", e))
            ))?
            .into_inner();

        let mut results = Vec::new();
        while let Some(grpc_block) = stream
            .message()
            .await
            .map_err(|e| LightweightWalletError::ScanningError(
                crate::errors::ScanningError::blockchain_connection_failed(&format!("Stream error: {}", e))
            ))?
        {
            if let Some(block_info) = Self::convert_block(&grpc_block) {
                results.push(block_info);
            }
        }

        Ok(results)
    }

    async fn get_block_by_height(
        &mut self,
        height: u64,
    ) -> LightweightWalletResult<Option<BlockInfo>> {
        let blocks = self.get_blocks_by_heights(vec![height]).await?;
        Ok(blocks.into_iter().next())
    }
}

#[cfg(feature = "grpc")]
impl std::fmt::Debug for GrpcBlockchainScanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcBlockchainScanner")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[cfg(feature = "grpc")]
impl Clone for GrpcBlockchainScanner {
    fn clone(&self) -> Self {
        // Note: This creates a new connection, which is expensive
        // In practice, you might want to use connection pooling
        panic!("GrpcBlockchainScanner cannot be cloned - create a new instance instead");
    }
}

/// Builder for creating GRPC blockchain scanners
#[cfg(feature = "grpc")]
pub struct GrpcScannerBuilder {
    base_url: Option<String>,
    timeout: Option<Duration>,
}

#[cfg(feature = "grpc")]
impl GrpcScannerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            base_url: None,
            timeout: None,
        }
    }

    /// Set the base URL for the GRPC connection
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = Some(base_url);
        self
    }

    /// Set the timeout for GRPC operations
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Build the GRPC scanner
    pub async fn build(self) -> LightweightWalletResult<GrpcBlockchainScanner> {
        let base_url = self.base_url
            .ok_or_else(|| LightweightWalletError::ConfigurationError(
                "Base URL not specified".to_string()
            ))?;

        match self.timeout {
            Some(timeout) => GrpcBlockchainScanner::with_timeout(base_url, timeout).await,
            None => GrpcBlockchainScanner::new(base_url).await,
        }
    }
}

#[cfg(feature = "grpc")]
impl Default for GrpcScannerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Placeholder module for when GRPC feature is not enabled
#[cfg(not(feature = "grpc"))]
pub struct GrpcBlockchainScanner;

#[cfg(not(feature = "grpc"))]
impl GrpcBlockchainScanner {
    pub async fn new(_base_url: String) -> crate::errors::LightweightWalletResult<Self> {
        Err(crate::errors::LightweightWalletError::OperationNotSupported(
            "GRPC feature not enabled".to_string()
        ))
    }
}

#[cfg(not(feature = "grpc"))]
pub struct GrpcScannerBuilder;

#[cfg(not(feature = "grpc"))]
impl GrpcScannerBuilder {
    pub fn new() -> Self {
        Self
    }

    pub async fn build(self) -> crate::errors::LightweightWalletResult<GrpcBlockchainScanner> {
        Err(crate::errors::LightweightWalletError::OperationNotSupported(
            "GRPC feature not enabled".to_string()
        ))
    }
}

#[cfg(feature = "grpc")]
#[async_trait::async_trait]
impl WalletScanner for GrpcBlockchainScanner {
    async fn scan_wallet(
        &mut self,
        config: WalletScanConfig,
    ) -> LightweightWalletResult<WalletScanResult> {
        DefaultScanningLogic::scan_wallet_with_progress(self, config, None).await
    }

    async fn scan_wallet_with_progress(
        &mut self,
        config: WalletScanConfig,
        progress_callback: Option<&ProgressCallback>,
    ) -> LightweightWalletResult<WalletScanResult> {
        DefaultScanningLogic::scan_wallet_with_progress(self, config, progress_callback).await
    }

    fn blockchain_scanner(&mut self) -> &mut dyn BlockchainScanner {
        self
    }
}

#[cfg(test)]

#[cfg(test)]
#[cfg(feature = "grpc")]
mod tests {
    use super::*;
    use crate::scanning::{ScanConfig, ExtractionConfig};

    #[tokio::test]
    async fn test_grpc_scanner_builder() {
        let builder = GrpcScannerBuilder::new()
            .with_base_url("http://127.0.0.1:18142".to_string())
            .with_timeout(std::time::Duration::from_secs(10));

        // This will fail if no base node is running, but that's expected
        let result = builder.build().await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[tokio::test]
    async fn test_grpc_scanner_creation() {
        let result = GrpcBlockchainScanner::new("http://127.0.0.1:18142".to_string()).await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[tokio::test]
    async fn test_grpc_scanner_with_timeout() {
        let result = GrpcBlockchainScanner::with_timeout(
            "http://127.0.0.1:18142".to_string(),
            std::time::Duration::from_secs(5)
        ).await;
        assert!(result.is_err()); // Should fail because no base node is running
    }

    #[test]
    fn test_grpc_scanner_debug() {
        // Test that the scanner can be created and debugged (even if connection fails)
        let scanner = GrpcBlockchainScanner;
        let debug_str = format!("{:?}", scanner);
        assert!(debug_str.contains("GrpcBlockchainScanner"));
    }
}

#[cfg(test)]
#[cfg(not(feature = "grpc"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_feature_disabled() {
        let result = GrpcBlockchainScanner::new("http://127.0.0.1:18142".to_string()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::LightweightWalletError::OperationNotSupported(_)
        ));
    }

    #[tokio::test]
    async fn test_grpc_builder_feature_disabled() {
        let builder = GrpcScannerBuilder::new();
        let result = builder.build().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::LightweightWalletError::OperationNotSupported(_)
        ));
    }
} 