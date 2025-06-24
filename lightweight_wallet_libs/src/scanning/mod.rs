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
    key_management::{KeyManager, KeyStore, KeyDerivationPath},
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

/// Configuration for wallet-specific scanning
pub struct WalletScanConfig {
    /// Base scan configuration
    pub scan_config: ScanConfig,
    /// Key manager for wallet key derivation
    pub key_manager: Option<Box<dyn KeyManager + Send + Sync>>,
    /// Key store for imported keys
    pub key_store: Option<KeyStore>,
    /// Whether to scan for stealth addresses
    pub scan_stealth_addresses: bool,
    /// Maximum number of addresses to scan per account
    pub max_addresses_per_account: u32,
    /// Whether to scan for imported keys
    pub scan_imported_keys: bool,
}

impl WalletScanConfig {
    /// Create a new wallet scan config
    pub fn new(start_height: u64) -> Self {
        Self {
            scan_config: ScanConfig {
                start_height,
                end_height: None,
                batch_size: 100,
                request_timeout: Duration::from_secs(30),
                extraction_config: ExtractionConfig::default(),
            },
            key_manager: None,
            key_store: None,
            scan_stealth_addresses: true,
            max_addresses_per_account: 1000,
            scan_imported_keys: true,
        }
    }

    /// Set the key manager
    pub fn with_key_manager(mut self, key_manager: Box<dyn KeyManager + Send + Sync>) -> Self {
        self.key_manager = Some(key_manager);
        self
    }

    /// Set the key store
    pub fn with_key_store(mut self, key_store: KeyStore) -> Self {
        self.key_store = Some(key_store);
        self
    }

    /// Set whether to scan for stealth addresses
    pub fn with_stealth_address_scanning(mut self, enabled: bool) -> Self {
        self.scan_stealth_addresses = enabled;
        self
    }

    /// Set maximum addresses per account
    pub fn with_max_addresses_per_account(mut self, max: u32) -> Self {
        self.max_addresses_per_account = max;
        self
    }

    /// Set whether to scan for imported keys
    pub fn with_imported_key_scanning(mut self, enabled: bool) -> Self {
        self.scan_imported_keys = enabled;
        self
    }

    /// Set the end height
    pub fn with_end_height(mut self, end_height: u64) -> Self {
        self.scan_config.end_height = Some(end_height);
        self
    }

    /// Set the batch size
    pub fn with_batch_size(mut self, batch_size: u64) -> Self {
        self.scan_config.batch_size = batch_size;
        self
    }

    /// Set the request timeout
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.scan_config.request_timeout = timeout;
        self
    }
}

/// Result of a wallet scan operation
#[derive(Debug, Clone)]
pub struct WalletScanResult {
    /// Block scan results
    pub block_results: Vec<BlockScanResult>,
    /// Total wallet outputs found
    pub total_wallet_outputs: u64,
    /// Total value found (in MicroMinotari)
    pub total_value: u64,
    /// Number of addresses scanned
    pub addresses_scanned: u64,
    /// Number of accounts scanned
    pub accounts_scanned: u64,
    /// Scan duration
    pub scan_duration: Duration,
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

/// Wallet scanner trait for scanning with wallet keys
/// 
/// This trait extends the basic blockchain scanner with wallet-specific
/// functionality for scanning with key management.
#[async_trait]
pub trait WalletScanner: Send + Sync {
    /// Scan for wallet outputs using wallet keys
    async fn scan_wallet(
        &mut self,
        config: WalletScanConfig,
    ) -> LightweightWalletResult<WalletScanResult>;

    /// Scan for wallet outputs with progress reporting
    async fn scan_wallet_with_progress(
        &mut self,
        config: WalletScanConfig,
        progress_callback: Option<&ProgressCallback>,
    ) -> LightweightWalletResult<WalletScanResult>;

    /// Get the underlying blockchain scanner
    fn blockchain_scanner(&mut self) -> &mut dyn BlockchainScanner;
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

    /// Process blocks with wallet key management
    pub fn process_blocks_with_wallet_keys(
        blocks: Vec<BlockInfo>,
        config: &WalletScanConfig,
    ) -> LightweightWalletResult<Vec<BlockScanResult>> {
        let mut results = Vec::new();

        for block in blocks {
            let mut wallet_outputs = Vec::new();
            
            for output in &block.outputs {
                    // TODO: Update to use new entropy-based key derivation approach
                // Try to extract wallet output with different key combinations
                if let Some(key_manager) = &config.key_manager {
                    // Try with derived keys
                    
                    for account in 0..=0 { // For now, just scan account 0
                        for change in 0..=1 { // External and internal addresses
                            for address_index in 0..config.max_addresses_per_account {
                                let path = KeyDerivationPath::tari_standard(account, change, address_index);
                                
                                // Derive the key pair for this path
                                match key_manager.derive_key_pair(&path) {
                                    Ok(key_pair) => {
                                        // Create extraction config with derived key
                                        let mut extraction_config = config.scan_config.extraction_config.clone();
                                        extraction_config.set_private_key(key_pair.private_key.clone());
                                        
                                        match extract_wallet_output(output, &extraction_config) {
                                            Ok(wallet_output) => {
                                                wallet_outputs.push(wallet_output);
                                                break; // Found a match, move to next output
                                            }
                                            Err(_) => {
                                                // Continue trying other keys
                                                continue;
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        // Skip this key if derivation failed
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                // Try with imported keys if enabled
                if config.scan_imported_keys {
                    if let Some(key_store) = &config.key_store {
                        for imported_key in key_store.get_imported_keys() {
                            // Create extraction config with imported key
                            let mut extraction_config = config.scan_config.extraction_config.clone();
                            extraction_config.set_private_key(imported_key.private_key.clone());
                            
                            match extract_wallet_output(output, &extraction_config) {
                                Ok(wallet_output) => {
                                    wallet_outputs.push(wallet_output);
                                    break; // Found a match, move to next output
                                }
                                Err(_) => {
                                    // Continue trying other keys
                                    continue;
                                }
                            }
                        }
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
        let mut all_results = Vec::new();
        let mut current_height = config.start_height;
        let end_height = config.end_height.unwrap_or_else(|| {
            // Get tip info if no end height specified
            // For now, we'll use a reasonable default
            current_height + 1000
        });

        while current_height <= end_height {
            let batch_end = std::cmp::min(current_height + config.batch_size - 1, end_height);
            
            // Get blocks in this batch
            let heights: Vec<u64> = (current_height..=batch_end).collect();
            let blocks = scanner.get_blocks_by_heights(heights).await?;
            
            // Process blocks
            let batch_results = Self::process_blocks(blocks, &config.extraction_config)?;
            all_results.extend(batch_results);

            // Update progress
            if let Some(callback) = progress_callback {
                let total_outputs: u64 = all_results.iter().map(|r| r.wallet_outputs.len() as u64).sum();
                let total_value: u64 = all_results.iter()
                    .flat_map(|r| &r.wallet_outputs)
                    .map(|wo| wo.value().as_u64())
                    .sum();

                callback(ScanProgress {
                    current_height: batch_end,
                    target_height: end_height,
                    outputs_found: total_outputs,
                    total_value,
                    elapsed: start_time.elapsed(),
                });
            }

            current_height = batch_end + 1;
        }

        Ok(all_results)
    }

    /// Scan wallet with progress reporting
    pub async fn scan_wallet_with_progress<S>(
        scanner: &mut S,
        config: WalletScanConfig,
        progress_callback: Option<&ProgressCallback>,
    ) -> LightweightWalletResult<WalletScanResult>
    where
        S: BlockchainScanner,
    {
        let start_time = Instant::now();
        let mut all_results = Vec::new();
        let mut current_height = config.scan_config.start_height;
        let end_height = config.scan_config.end_height.unwrap_or_else(|| {
            // Get tip info if no end height specified
            // For now, we'll use a reasonable default
            current_height + 1000
        });

        while current_height <= end_height {
            let batch_end = std::cmp::min(current_height + config.scan_config.batch_size - 1, end_height);
            
            // Get blocks in this batch
            let heights: Vec<u64> = (current_height..=batch_end).collect();
            let blocks = scanner.get_blocks_by_heights(heights).await?;
            
            // Process blocks with wallet keys
            let batch_results = Self::process_blocks_with_wallet_keys(blocks, &config)?;
            all_results.extend(batch_results);

            // Update progress
            if let Some(callback) = progress_callback {
                let total_outputs: u64 = all_results.iter().map(|r| r.wallet_outputs.len() as u64).sum();
                let total_value: u64 = all_results.iter()
                    .flat_map(|r| &r.wallet_outputs)
                    .map(|wo| wo.value().as_u64())
                    .sum();

                callback(ScanProgress {
                    current_height: batch_end,
                    target_height: end_height,
                    outputs_found: total_outputs,
                    total_value,
                    elapsed: start_time.elapsed(),
                });
            }

            current_height = batch_end + 1;
        }

        let total_wallet_outputs: u64 = all_results.iter().map(|r| r.wallet_outputs.len() as u64).sum();
        let total_value: u64 = all_results.iter()
            .flat_map(|r| &r.wallet_outputs)
            .map(|wo| wo.value().as_u64())
            .sum();

        Ok(WalletScanResult {
            block_results: all_results,
            total_wallet_outputs,
            total_value,
            addresses_scanned: 0, // TODO: Calculate this
            accounts_scanned: 0,  // TODO: Calculate this
            scan_duration: start_time.elapsed(),
        })
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