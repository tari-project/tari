// Copyright 2025. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{fs, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use tari_common::configuration::Network;
use tari_common_types::{
    seeds::{cipher_seed::CipherSeed, mnemonic::Mnemonic, seed_words::SeedWords},
    types::PrivateKey,
};
use tari_transaction_components::{
    consensus::ConsensusManager,
    key_manager::{
        wallet_types::{SeedWordsWallet, SpendWallet, WalletType},
        KeyManager,
    },
    offline_signing::{
        models::{PrepareOneSidedTransactionForSigningResult, TransactionResult},
        sign_locked_transaction,
    },
};
use tari_utilities::{hex::Hex, SafePassword};

use crate::{error::OfflineSignerError, keystore};

/// Minotari Offline Transaction Signer
///
/// A standalone tool for signing one-sided transactions offline using private keys.
/// Keys are securely stored in the OS keystore, encrypted with a passphrase.
#[derive(Debug, Parser)]
#[clap(name = "minotari_offline_signer", version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize the offline signer with keys
    Init {
        #[clap(subcommand)]
        method: InitMethod,
    },

    /// Sign a one-sided transaction using stored keys
    Sign(SignArgs),

    /// Check if the signer has been initialized
    Status,

    /// Clear all stored keys from the keystore
    Clear,
}

#[derive(Debug, Subcommand, Clone)]
pub enum InitMethod {
    /// Initialize with spend and view keys directly
    Keys(InitKeysArgs),

    /// Initialize with seed words (mnemonic phrase)
    SeedWords(InitSeedWordsArgs),
}

#[derive(Debug, Args, Clone)]
pub struct InitKeysArgs {
    /// Private spend key in hexadecimal format
    #[clap(long, env = "TARI_SPEND_KEY")]
    pub spend_key: String,

    /// Private view key in hexadecimal format
    #[clap(long, env = "TARI_VIEW_KEY")]
    pub view_key: String,

    /// Passphrase to encrypt the keys
    #[clap(long, env = "TARI_PASSPHRASE")]
    pub passphrase: String,
}

#[derive(Debug, Args, Clone)]
pub struct InitSeedWordsArgs {
    /// Seed words (mnemonic phrase) separated by spaces
    #[clap(long, env = "TARI_SEED_WORDS")]
    pub seed_words: Option<String>,

    /// Optional passphrase for the seed words (BIP39 passphrase, not the encryption passphrase)
    #[clap(long, env = "TARI_SEED_PASSPHRASE")]
    pub seed_passphrase: Option<String>,

    /// Passphrase to encrypt the keys in the keystore
    #[clap(long, env = "TARI_PASSPHRASE")]
    pub passphrase: String,
}

#[derive(Debug, Args, Clone)]
pub struct SignArgs {
    /// Path to the input file containing the prepared transaction (JSON format)
    #[clap(short, long)]
    pub input_file: PathBuf,

    /// Path to the output file for the signed transaction (JSON format)
    #[clap(short, long)]
    pub output_file: PathBuf,

    /// Passphrase to decrypt the keys
    #[clap(long, env = "TARI_PASSPHRASE")]
    pub passphrase: String,

    /// Network to use (mainnet, nextnet, esmeralda, localnet)
    #[clap(short, long, default_value = "mainnet")]
    pub network: String,
}

impl Cli {
    pub fn execute(self) -> Result<()> {
        match self.command {
            Commands::Init { method } => match method {
                InitMethod::Keys(args) => init_with_keys(args),
                InitMethod::SeedWords(args) => init_with_seed_words(args),
            },
            Commands::Sign(args) => sign_transaction(args),
            Commands::Status => check_status(),
            Commands::Clear => clear_keystore(),
        }
    }
}

fn check_already_initialized() -> Result<()> {
    if keystore::is_initialized() {
        return Err(anyhow!(
            "Keystore is already initialized. Use 'clear' command first to reinitialize."
        ));
    }
    Ok(())
}

fn init_with_keys(args: InitKeysArgs) -> Result<()> {
    check_already_initialized()?;

    // Parse the private keys from hex
    let spend_key = PrivateKey::from_hex(&args.spend_key)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Invalid spend key: {}", e)))?;
    let view_key = PrivateKey::from_hex(&args.view_key)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Invalid view key: {}", e)))?;

    // Get passphrase (prompt if not provided)
    let passphrase = args.passphrase;

    // Initialize the keystore
    keystore::init_keystore(&spend_key, &view_key, &passphrase)?;

    println!("✓ Offline signer initialized successfully!");
    println!("  Keys have been encrypted and stored in the OS keystore.");

    Ok(())
}

fn init_with_seed_words(args: InitSeedWordsArgs) -> Result<()> {
    check_already_initialized()?;

    // Get seed words (prompt if not provided)
    let seed_words_str = match args.seed_words {
        Some(s) => s,
        None => prompt_seed_words()?,
    };

    // Parse seed words
    let seed_words = SeedWords::from_str(&seed_words_str)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Invalid seed words: {}", e)))?;

    // Convert optional seed passphrase to SafePassword
    let seed_passphrase = args.seed_passphrase.map(|p| SafePassword::from(p));

    // Derive cipher seed from mnemonic
    let cipher_seed = CipherSeed::from_mnemonic(&seed_words, seed_passphrase)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Failed to derive seed from mnemonic: {}", e)))?;

    // Create SeedWordsWallet to derive spend and view keys
    let seed_wallet = SeedWordsWallet::construct_new(cipher_seed)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Failed to derive keys from seed: {}", e)))?;

    let spend_key = seed_wallet.spend_key().clone();
    let view_key = seed_wallet.view_key().clone();

    // Get encryption passphrase (prompt if not provided)
    let passphrase = args.passphrase;

    // Initialize the keystore
    keystore::init_keystore(&spend_key, &view_key, &passphrase)?;

    println!("✓ Offline signer initialized successfully from seed words!");
    println!("  Keys have been derived and encrypted in the OS keystore.");

    Ok(())
}

fn sign_transaction(args: SignArgs) -> Result<()> {
    // Check if initialized
    if !keystore::is_initialized() {
        return Err(anyhow!("Offline signer not initialized. Run 'init' command first."));
    }

    // Get passphrase (prompt if not provided)
    let passphrase = args.passphrase;

    // Retrieve keys from keystore
    let (spend_key, view_key) =
        keystore::get_keys(&passphrase).map_err(|e| anyhow!("Failed to retrieve keys: {}", e))?;

    // Parse the network
    let network = Network::from_str(&args.network).map_err(|_| {
        anyhow!(
            "Invalid network '{}'. Valid options: mainnet, nextnet, esmeralda, localnet",
            args.network
        )
    })?;

    // Validate input file
    let metadata =
        fs::metadata(&args.input_file).with_context(|| format!("Failed to read input file: {:?}", args.input_file))?;

    const MAX_FILE_SIZE: u64 = 10_000_000; // 10MB limit
    if metadata.len() > MAX_FILE_SIZE {
        return Err(anyhow!("Input file too large (max {} bytes)", MAX_FILE_SIZE));
    }

    // Read and parse the input file
    let data = fs::read_to_string(&args.input_file)
        .with_context(|| format!("Failed to read input file: {:?}", args.input_file))?;

    let request = PrepareOneSidedTransactionForSigningResult::from_json(&data)
        .map_err(|e| OfflineSignerError::ParseError(format!("Failed to parse transaction: {}", e)))?;

    // Create the key manager with the spend wallet type
    let spend_wallet = SpendWallet::new(spend_key, view_key, None);
    let wallet_type = WalletType::SpendWallet(spend_wallet);
    let key_manager = KeyManager::new(wallet_type)
        .map_err(|e| OfflineSignerError::KeyManagerError(format!("Failed to create key manager: {}", e)))?;

    // Get consensus constants for the network
    let consensus_manager = ConsensusManager::builder(network).build();
    let consensus_constants = consensus_manager.consensus_constants(0).clone();

    // Sign the transaction
    let result = sign_locked_transaction(&key_manager, consensus_constants, network, request)
        .map_err(|e| OfflineSignerError::SigningError(format!("Failed to sign transaction: {}", e)))?;

    // Write the signed transaction to the output file
    let json_data = result
        .to_json()
        .map_err(|e| OfflineSignerError::SerializationError(format!("Failed to serialize result: {}", e)))?;

    fs::write(&args.output_file, json_data)
        .with_context(|| format!("Failed to write output file: {:?}", args.output_file))?;

    println!("✓ Transaction signed successfully!");
    println!("  Output written to: {:?}", args.output_file);

    Ok(())
}

fn check_status() -> Result<()> {
    if keystore::is_initialized() {
        println!("✓ Offline signer is initialized");
        println!("  Keys are stored in the OS keystore");
    } else {
        println!("✗ Offline signer is not initialized");
        println!("  Run 'init keys' or 'init seed-words' command to set up keys");
    }
    Ok(())
}

fn clear_keystore() -> Result<()> {
    if !keystore::is_initialized() {
        println!("Keystore is already empty");
        return Ok(());
    }

    // Prompt for confirmation
    println!("WARNING: This will permanently delete all stored keys!");
    print!("Are you sure you want to continue? [y/N]: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "y" {
        println!("Aborted");
        return Ok(());
    }

    keystore::clear_keystore()?;
    println!("✓ Keystore cleared successfully");

    Ok(())
}
