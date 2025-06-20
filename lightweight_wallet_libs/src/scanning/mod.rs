// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! UTXO scanning module for lightweight wallet libraries
//! 
//! This module provides a lightweight interface for scanning the Tari blockchain
//! for wallet outputs. It uses a trait-based approach that allows different
//! backend implementations (gRPC, HTTP, etc.) to be plugged in.

use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::{
    data_structures::{transaction_output::LightweightTransactionOutput, wallet_output::LightweightWalletOutput},
    errors::{LightweightWalletError, LightweightWalletResult},
    extraction::{extract_wallet_output, ExtractionConfig},
};

// Include GRPC scanner when the feature is enabled
#[cfg(feature = "grpc")]
pub mod grpc_scanner;

// Re-export GRPC scanner types
#[cfg(feature = "grpc")]
pub use grpc_scanner::{GrpcBlockchainScanner, GrpcScannerBuilder};

/// Progress callback for scanning operations
pub type ProgressCallback = Box<dyn Fn(ScanProgress) + Send + Sync>;

/// Scanning progress information
#[derive(Debug, Clone)]
pub struct ScanProgress {
    /// Current block height being scanned
    pub current_height: u64,
    /// Target block height to scan to
    pub target_height: u64,
    /// Number of outputs found so far
    pub outputs_found: u64,
    /// Total value of outputs found so far (in MicroMinotari)
    pub total_value: u64,
    /// Time elapsed since scan started
    pub elapsed: Duration,
}

/// Result of a block scan operation
#[derive(Debug, Clone)]
pub struct BlockScanResult {
    /// Block height
    pub height: u64,
    /// Block hash
    pub block_hash: Vec<u8>,
    /// Transaction outputs found in this block
    pub outputs: Vec<LightweightTransactionOutput>,
    /// Wallet outputs extracted from transaction outputs
    pub wallet_outputs: Vec<LightweightWalletOutput>,
    /// Timestamp when block was mined
    pub mined_timestamp: u64,
}

/// Configuration for blockchain scanning
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Starting block height (wallet birthday)
    pub start_height: u64,
    /// Ending block height (optional, if None scans to tip)
    pub end_height: Option<u64>,
    /// Maximum number of blocks to scan in one request
    pub batch_size: u64,
    /// Timeout for requests
    pub request_timeout: Duration,
    /// Extraction configuration
    pub extraction_config: ExtractionConfig,
}

impl ScanConfig {
    /// Create a new scan config with a progress callback
    pub fn with_progress_callback(
        self,
        callback: ProgressCallback,
    ) -> ScanConfigWithCallback {
        ScanConfigWithCallback {
            config: self,
            progress_callback: Some(callback),
        }
    }
}

/// Scan config with progress callback (not Debug/Clone)
pub struct ScanConfigWithCallback {
    pub config: ScanConfig,
    pub progress_callback: Option<ProgressCallback>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            start_height: 0,
            end_height: None,
            batch_size: 100,
            request_timeout: Duration::from_secs(30),
            extraction_config: ExtractionConfig::default(),
        }
    }
}

/// Chain tip information
#[derive(Debug, Clone)]
pub struct TipInfo {
    /// Current best block height
    pub best_block_height: u64,
    /// Current best block hash
    pub best_block_hash: Vec<u8>,
    /// Accumulated difficulty
    pub accumulated_difficulty: Vec<u8>,
    /// Pruned height (minimum height this node can provide complete blocks for)
    pub pruned_height: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Block information
#[derive(Debug, Clone)]
pub struct BlockInfo {
    /// Block height
    pub height: u64,
    /// Block hash
    pub hash: Vec<u8>,
    /// Block timestamp
    pub timestamp: u64,
    /// Transaction outputs in this block
    pub outputs: Vec<LightweightTransactionOutput>,
}

/// Blockchain scanner trait for scanning UTXOs
/// 
/// This trait provides a lightweight interface that can be implemented by
/// different backend providers (gRPC, HTTP, etc.) without requiring heavy
/// dependencies in the core library.
#[async_trait]
pub trait BlockchainScanner: Send + Sync {
    /// Scan for wallet outputs in the specified block range
    async fn scan_blocks(
        &mut self,
        config: ScanConfig,
    ) -> LightweightWalletResult<Vec<BlockScanResult>>;

    /// Get the current chain tip information
    async fn get_tip_info(&mut self) -> LightweightWalletResult<TipInfo>;

    /// Search for specific UTXOs by commitment
    async fn search_utxos(
        &mut self,
        commitments: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<BlockScanResult>>;

    /// Fetch specific UTXOs by hash
    async fn fetch_utxos(
        &mut self,
        hashes: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<LightweightTransactionOutput>>;

    /// Get blocks by height range
    async fn get_blocks_by_heights(
        &mut self,
        heights: Vec<u64>,
    ) -> LightweightWalletResult<Vec<BlockInfo>>;

    /// Get a single block by height
    async fn get_block_by_height(
        &mut self,
        height: u64,
    ) -> LightweightWalletResult<Option<BlockInfo>>;
}

/// Default implementation of scanning logic that can be used by any backend
pub struct DefaultScanningLogic;

impl DefaultScanningLogic {
    /// Process blocks and extract wallet outputs
    pub fn process_blocks(
        blocks: Vec<BlockInfo>,
        extraction_config: &ExtractionConfig,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        let mut results = Vec::new();

        for block in blocks {
            let mut wallet_outputs = Vec::new();
            
            for output in &block.outputs {
                match extract_wallet_output(output, extraction_config) {
                    Ok(wallet_output) => wallet_outputs.push(wallet_output),
                    Err(e) => {
                        // Log error but continue processing other outputs
                        tracing::debug!("Failed to extract wallet output: {}", e);
                    }
                }
            }

            results.push(BlockScanResult {
                height: block.height,
                block_hash: block.hash,
                outputs: block.outputs,
                wallet_outputs,
                mined_timestamp: block.timestamp,
            });
        }

        Ok(results)
    }

    /// Scan blocks with progress reporting
    pub async fn scan_blocks_with_progress<S>(
        scanner: &mut S,
        config: ScanConfig,
        progress_callback: Option<&ProgressCallback>,
    ) -> LightweightWalletResult<Vec<BlockScanResult>>
    where
        S: BlockchainScanner,
    {
        let start_time = Instant::now();
        let mut results = Vec::new();
        let mut current_height = config.start_height;

        // Get tip info to determine end height
        let tip_info = scanner.get_tip_info().await?;
        let end_height = config.end_height.unwrap_or(tip_info.best_block_height);

        if current_height > end_height {
            return Ok(results);
        }

        let mut outputs_found = 0u64;
        let mut total_value = 0u64;

        while current_height <= end_height {
            let batch_end = std::cmp::min(current_height + config.batch_size - 1, end_height);
            let heights: Vec<u64> = (current_height..=batch_end).collect();

            // Get blocks for this batch
            let blocks = scanner.get_blocks_by_heights(heights).await?;

            // Process each block
            for block in blocks {
                let mut wallet_outputs = Vec::new();
                
                for output in &block.outputs {
                    match extract_wallet_output(output, &config.extraction_config) {
                        Ok(wallet_output) => wallet_outputs.push(wallet_output),
                        Err(e) => {
                            tracing::debug!("Failed to extract wallet output: {}", e);
                        }
                    }
                }

                outputs_found += wallet_outputs.len() as u64;
                total_value += wallet_outputs.iter()
                    .map(|wo| wo.value().as_u64())
                    .sum::<u64>();

                results.push(BlockScanResult {
                    height: block.height,
                    block_hash: block.hash,
                    outputs: block.outputs,
                    wallet_outputs,
                    mined_timestamp: block.timestamp,
                });
            }

            // Update progress
            if let Some(callback) = progress_callback {
                let progress = ScanProgress {
                    current_height: batch_end,
                    target_height: end_height,
                    outputs_found,
                    total_value,
                    elapsed: start_time.elapsed(),
                };
                callback(progress);
            }

            current_height = batch_end + 1;
        }

        Ok(results)
    }
}

/// Mock implementation for testing
pub struct MockBlockchainScanner {
    blocks: Vec<BlockInfo>,
    tip_info: TipInfo,
}

impl MockBlockchainScanner {
    /// Create a new mock scanner
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            tip_info: TipInfo {
                best_block_height: 1000,
                best_block_hash: vec![1, 2, 3, 4],
                accumulated_difficulty: vec![5, 6, 7, 8],
                pruned_height: 500,
                timestamp: 1234567890,
            },
        }
    }

    /// Add a mock block
    pub fn add_block(&mut self, block: BlockInfo) {
        self.blocks.push(block);
    }

    /// Set tip info
    pub fn set_tip_info(&mut self, tip_info: TipInfo) {
        self.tip_info = tip_info;
    }
}

#[async_trait]
impl BlockchainScanner for MockBlockchainScanner {
    async fn scan_blocks(
        &mut self,
        config: ScanConfig,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        DefaultScanningLogic::scan_blocks_with_progress(self, config, None).await
    }

    async fn get_tip_info(&mut self) -> LightweightWalletResult<TipInfo> {
        Ok(self.tip_info.clone())
    }

    async fn search_utxos(
        &mut self,
        _commitments: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        // Mock implementation - return empty results
        Ok(Vec::new())
    }

    async fn fetch_utxos(
        &mut self,
        _hashes: Vec<Vec<u8>>,
    ) -> LightweightWalletResult<Vec<LightweightTransactionOutput>> {
        // Mock implementation - return empty results
        Ok(Vec::new())
    }

    async fn get_blocks_by_heights(
        &mut self,
        heights: Vec<u64>,
    ) -> LightweightWalletResult<Vec<BlockInfo>> {
        let mut result = Vec::new();
        for height in heights {
            if let Some(block) = self.blocks.iter().find(|b| b.height == height) {
                result.push(block.clone());
            }
        }
        Ok(result)
    }

    async fn get_block_by_height(
        &mut self,
        height: u64,
    ) -> LightweightWalletResult<Option<BlockInfo>> {
        Ok(self.blocks.iter().find(|b| b.height == height).cloned())
    }
}

/// Builder for creating blockchain scanners
pub struct BlockchainScannerBuilder {
    scanner_type: Option<ScannerType>,
    config: Option<ScannerConfig>,
}

#[derive(Debug, Clone)]
pub enum ScannerType {
    Mock,
    // Add other scanner types here as needed
    #[cfg(feature = "grpc")]
    Grpc { url: String },
    // Http { url: String },
}

#[derive(Debug, Clone)]
pub struct ScannerConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub retry_attempts: u32,
}

impl BlockchainScannerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            scanner_type: None,
            config: None,
        }
    }

    /// Set the scanner type
    pub fn with_type(mut self, scanner_type: ScannerType) -> Self {
        self.scanner_type = Some(scanner_type);
        self
    }

    /// Set the scanner configuration
    pub fn with_config(mut self, config: ScannerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the scanner
    pub async fn build(self) -> LightweightWalletResult<Box<dyn BlockchainScanner>> {
        match self.scanner_type {
            Some(ScannerType::Mock) => Ok(Box::new(MockBlockchainScanner::new())),
            #[cfg(feature = "grpc")]
            Some(ScannerType::Grpc { url }) => {
                let scanner = GrpcBlockchainScanner::new(url).await?;
                Ok(Box::new(scanner))
            }
            None => Err(LightweightWalletError::ConfigurationError(
                "Scanner type not specified".to_string()
            )),
        }
    }
}

impl Default for BlockchainScannerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::MicroMinotari;

    #[tokio::test]
    async fn test_scan_config_default() {
        let config = ScanConfig::default();
        assert_eq!(config.start_height, 0);
        assert_eq!(config.end_height, None);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert!(config.extraction_config.enable_key_derivation);
    }

    #[tokio::test]
    async fn test_scan_progress() {
        let progress = ScanProgress {
            current_height: 1000,
            target_height: 2000,
            outputs_found: 5,
            total_value: 1000000,
            elapsed: Duration::from_secs(10),
        };

        assert_eq!(progress.current_height, 1000);
        assert_eq!(progress.target_height, 2000);
        assert_eq!(progress.outputs_found, 5);
        assert_eq!(progress.total_value, 1000000);
        assert_eq!(progress.elapsed, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_block_scan_result() {
        let result = BlockScanResult {
            height: 1000,
            block_hash: vec![1, 2, 3, 4],
            outputs: vec![],
            wallet_outputs: vec![],
            mined_timestamp: 1234567890,
        };

        assert_eq!(result.height, 1000);
        assert_eq!(result.block_hash, vec![1, 2, 3, 4]);
        assert_eq!(result.mined_timestamp, 1234567890);
        assert!(result.outputs.is_empty());
        assert!(result.wallet_outputs.is_empty());
    }

    #[tokio::test]
    async fn test_tip_info() {
        let tip_info = TipInfo {
            best_block_height: 1000,
            best_block_hash: vec![1, 2, 3, 4],
            accumulated_difficulty: vec![5, 6, 7, 8],
            pruned_height: 500,
            timestamp: 1234567890,
        };

        assert_eq!(tip_info.best_block_height, 1000);
        assert_eq!(tip_info.best_block_hash, vec![1, 2, 3, 4]);
        assert_eq!(tip_info.accumulated_difficulty, vec![5, 6, 7, 8]);
        assert_eq!(tip_info.pruned_height, 500);
        assert_eq!(tip_info.timestamp, 1234567890);
    }

    #[tokio::test]
    async fn test_mock_scanner() {
        let mut scanner = MockBlockchainScanner::new();
        let tip_info = scanner.get_tip_info().await.unwrap();
        assert_eq!(tip_info.best_block_height, 1000);
    }

    #[tokio::test]
    async fn test_scanner_builder() {
        let builder = BlockchainScannerBuilder::new()
            .with_type(ScannerType::Mock);
        
        let mut scanner = builder.build().await.unwrap();
        let tip_info = scanner.get_tip_info().await.unwrap();
        assert_eq!(tip_info.best_block_height, 1000);
    }
} 