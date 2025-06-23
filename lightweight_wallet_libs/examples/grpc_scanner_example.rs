// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Example demonstrating how to use the GRPC blockchain scanner
//! 
//! This example shows how to connect to a Tari base node via GRPC
//! and scan for wallet outputs using wallet keys.

#[cfg(feature = "grpc")]
use lightweight_wallet_libs::{
    scanning::{GrpcScannerBuilder, WalletScanConfig, WalletScanner, ProgressCallback, ScanProgress},
    extraction::ExtractionConfig,
    key_derivation::LightweightKeyManager,
    key_management::{ConcreteKeyManager, KeyStore, ImportedPrivateKey, KeyDerivationPath, KeyManager},
    BlockchainScanner,
    errors::LightweightWalletResult,
};
use tracing_subscriber::fmt;

#[cfg(feature = "grpc")]
#[tokio::main]
async fn main() -> LightweightWalletResult<()> {
    // Initialize logging

    tracing_subscriber::fmt::init();

    println!("GRPC Wallet Scanner Example");
    println!("===========================");

    // Create a GRPC scanner builder
    let builder = GrpcScannerBuilder::new()
        .with_base_url("http://127.0.0.1:18142".to_string())
        .with_timeout(std::time::Duration::from_secs(30));

    // Build the scanner
    let mut scanner = match builder.build().await {
        Ok(scanner) => scanner,
        Err(e) => {
            eprintln!("Failed to create GRPC scanner: {}", e);
            eprintln!("Make sure a Tari base node is running with GRPC enabled on port 18142");
            return Err(e);
        }
    };

    println!("Connected to base node successfully!");

    // Get tip information
    let tip_info = scanner.get_tip_info().await?;
    println!("Current tip height: {}", tip_info.best_block_height);
    println!("Current tip hash: {}", hex::encode(&tip_info.best_block_hash));

    // Create a key manager (example with a test key)
    // In a real application, this would be created from a seed phrase or imported keys
    let test_master_key = [1u8; 32]; // Example key - in real usage, this would be a proper seed
    let key_manager = ConcreteKeyManager::new(test_master_key);
    
    // Create a key store with some imported keys (example)
    let mut key_store = KeyStore::new();
    
    // Example: Create an imported key from a seed phrase using the new convenience function
    // Extract the seed from env
    let seed_phrase = std::env::var("TEST_SEED_PHRASE").expect("Define TEST_SEED_PHRASE").to_string();
    
    // Use the new convenience function to get a private key directly from seed phrase
    let private_key = lightweight_wallet_libs::key_management::private_key_from_seed_phrase(
        &seed_phrase,
        None
    ).unwrap();
    
    // Create the ImportedPrivateKey
    let test_imported_key = ImportedPrivateKey::from_seed_phrase(
        private_key,
        KeyDerivationPath::default(),
        Some("test_imported_key".to_string())
    );
    key_store.add_imported_key(test_imported_key).unwrap();

    // Configure wallet scanning
    let wallet_birthday = 950;//tip_info.best_block_height.saturating_sub(1000); // Example: scan from 1000 blocks ago
    let tip_end_height = 960;// tip_info.best_block_height;
    let wallet_scan_config = WalletScanConfig::new(wallet_birthday)
        .with_key_manager(key_manager)
        .with_key_store(key_store)
        .with_stealth_address_scanning(true)
        .with_max_addresses_per_account(1) // Scan first 100 addresses per account
        .with_imported_key_scanning(true)
        .with_end_height(tip_end_height)
        .with_batch_size(100)
        .with_request_timeout(std::time::Duration::from_secs(30));

    println!("Wallet birthday: {}", wallet_birthday);
    println!("Scanning blocks from {} to {}", wallet_birthday, tip_end_height);
    println!("Scanning for wallet outputs with key management...");

    // Create a progress callback
    let progress_callback: ProgressCallback = Box::new(|progress: ScanProgress| {
        println!(
            "Progress: {}/{} blocks ({} outputs, {} MicroMinotari) - {:.2}s elapsed",
            progress.current_height,
            progress.target_height,
            progress.outputs_found,
            progress.total_value,
            progress.elapsed.as_secs_f64()
        );
    });

    // Scan for wallet outputs with progress reporting
    let wallet_result = scanner.scan_wallet_with_progress(
        wallet_scan_config,
        Some(&progress_callback)
    ).await?;
    
    println!("\nWallet scan completed!");
    println!("=====================");
    println!("Total blocks scanned: {}", wallet_result.block_results.len());
    println!("Total wallet outputs found: {}", wallet_result.total_wallet_outputs);
    println!("Total value found: {} MicroMinotari", wallet_result.total_value);
    println!("Addresses scanned: {}", wallet_result.addresses_scanned);
    println!("Accounts scanned: {}", wallet_result.accounts_scanned);
    println!("Scan duration: {:.2}s", wallet_result.scan_duration.as_secs_f64());

    // Print details of blocks with wallet outputs
    let blocks_with_outputs: Vec<_> = wallet_result.block_results
        .iter()
        .filter(|r| !r.wallet_outputs.is_empty())
        .collect();

    if !blocks_with_outputs.is_empty() {
        println!("\nBlocks with wallet outputs:");
        println!("===========================");
        for result in blocks_with_outputs {
            let block_value: u64 = result.wallet_outputs.iter()
                .map(|wo| wo.value().as_u64())
                .sum();
            
            println!("Block {}: {} outputs, {} MicroMinotari", 
                result.height, 
                result.wallet_outputs.len(),
                block_value
            );
            
            // Print details of each wallet output
            for (i, wallet_output) in result.wallet_outputs.iter().enumerate() {
                println!("  Output {}: {} MicroMinotari, Payment ID: {:?}", 
                    i + 1,
                    wallet_output.value().as_u64(),
                    wallet_output.payment_id()
                );
            }
        }
    } else {
        println!("\nNo wallet outputs found in the scanned range.");
        println!("This could mean:");
        println!("1. The wallet has no outputs in this block range");
        println!("2. The keys provided don't match any outputs");
        println!("3. The wallet birthday is set too high");
        println!("4. The base node doesn't have the required blocks");
    }

    // Demonstrate basic scanning without key management (for comparison)
    println!("\nPerforming basic scan without key management...");
    
    let basic_extraction_config = ExtractionConfig {
        enable_key_derivation: false,
        validate_range_proofs: false,
        validate_signatures: false,
        handle_special_outputs: true,
        detect_corruption: false,
        private_key: None,
        public_key: None,
    };

    let basic_scan_config = lightweight_wallet_libs::scanning::ScanConfig {
        start_height: tip_info.best_block_height.saturating_sub(10), // Just last 10 blocks
        end_height: Some(tip_info.best_block_height),
        batch_size: 10,
        request_timeout: std::time::Duration::from_secs(30),
        extraction_config: basic_extraction_config,
    };

    let basic_results = scanner.scan_blocks(basic_scan_config).await?;
    let basic_total_outputs: usize = basic_results.iter().map(|r| r.wallet_outputs.len()).sum();
    
    println!("Basic scan found {} wallet outputs (without key management)", basic_total_outputs);
    println!("Note: Without proper keys, most outputs cannot be extracted as wallet outputs.");

    Ok(())
}

#[cfg(not(feature = "grpc"))]
fn main() {
    eprintln!("This example requires the 'grpc' feature to be enabled.");
    eprintln!("Run with: cargo run --example grpc_scanner_example --features grpc");
    std::process::exit(1);
} 