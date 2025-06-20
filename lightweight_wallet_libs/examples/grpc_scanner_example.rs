// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Example demonstrating how to use the GRPC blockchain scanner
//! 
//! This example shows how to connect to a Tari base node via GRPC
//! and scan for wallet outputs.

#[cfg(feature = "grpc")]
use lightweight_wallet_libs::{
    scanning::{GrpcScannerBuilder, ScanConfig},
    extraction::ExtractionConfig,
    BlockchainScanner,
    errors::LightweightWalletResult,
};
use tracing_subscriber::fmt;

#[cfg(feature = "grpc")]
#[tokio::main]
async fn main() -> LightweightWalletResult<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("GRPC Scanner Example");
    println!("===================");

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

    // Configure scanning
    let extraction_config = ExtractionConfig {
        enable_key_derivation: true,
        validate_range_proofs: true,
        validate_signatures: true,
        handle_special_outputs: true,
        detect_corruption: true,
    };

    let scan_config = ScanConfig {
        start_height: tip_info.best_block_height.saturating_sub(100), // Scan last 100 blocks
        end_height: Some(tip_info.best_block_height),
        batch_size: 10,
        request_timeout: std::time::Duration::from_secs(30),
        extraction_config,
    };

    println!("Scanning blocks from {} to {}", scan_config.start_height, scan_config.end_height.unwrap());

    // Scan for wallet outputs
    let results = scanner.scan_blocks(scan_config).await?;
    
    println!("Scan completed!");
    println!("Found {} blocks with wallet outputs", results.len());

    let total_outputs: usize = results.iter().map(|r| r.wallet_outputs.len()).sum();
    let total_value: u64 = results.iter()
        .flat_map(|r| &r.wallet_outputs)
        .map(|wo| wo.value().as_u64())
        .sum();

    println!("Total wallet outputs found: {}", total_outputs);
    println!("Total value found: {} MicroMinotari", total_value);

    // Print details of each block with outputs
    for result in &results {
        if !result.wallet_outputs.is_empty() {
            println!("Block {}: {} outputs, {} MicroMinotari", 
                result.height, 
                result.wallet_outputs.len(),
                result.wallet_outputs.iter().map(|wo| wo.value().as_u64()).sum::<u64>()
            );
        }
    }

    Ok(())
}

#[cfg(not(feature = "grpc"))]
fn main() {
    eprintln!("This example requires the 'grpc' feature to be enabled.");
    eprintln!("Run with: cargo run --example grpc_scanner_example --features grpc");
    std::process::exit(1);
} 