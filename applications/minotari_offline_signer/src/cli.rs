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

use std::{
    ffi::OsString,
    fs,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use tari_common::{configuration::Network, network_check::set_network_if_choice_valid};
use tari_common_types::{
    seeds::{cipher_seed::CipherSeed, mnemonic::Mnemonic, seed_words::SeedWords},
    types::{CompressedPublicKey, PrivateKey},
};
use tari_transaction_components::{
    consensus::ConsensusManager,
    key_manager::{
        KeyManager,
        SecretTransactionKeyManagerInterface,
        TariKeyId,
        wallet_types::{SeedWordsWallet, SpendWallet, WalletType},
    },
    offline_signing::{
        ConfirmationMode,
        ConfirmedPayload,
        PayloadKeyPolicy,
        PayloadSummary,
        models::{ExportedPayloadKey, PrepareOneSidedTransactionForSigningResult, TransactionResult},
        present_payload_summary,
        sign_locked_transaction,
        verify_offline_payload,
    },
};
use tari_utilities::{SafePassword, hex::Hex};

use crate::{error::OfflineSignerError, keystore};

/// Minotari Offline Transaction Signer
///
/// A standalone tool for signing one-sided transactions offline using private keys.
/// Keys are securely stored in the OS keystore, encrypted with a passphrase.
#[derive(Debug, Parser)]
#[clap(name = "minotari_offline_signer", version, about)]
pub struct Cli {
    /// Use a file-backed keystore instead of the OS keyring.
    #[cfg(feature = "test-keystore")]
    #[clap(long, global = true, hide = true)]
    pub test_keystore_file: Option<PathBuf>,

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
    ///
    /// Only transactions that leave a change output are signed. A payload where what is left after the outputs and
    /// the fee is no more than a change output would cost, such as a send-all or sweep, is refused: with no change
    /// output, the script offset of the signed transaction together
    /// with what the online (view key) wallet already knows would let it work out the spend key. Prepare it again
    /// with a slightly smaller amount. This closes one known recovery path only; a view key holder can still produce
    /// a payload that passes verification, so check every recipient, amount and "Other outputs" entry, and the fee,
    /// total spend and change, before approving.
    Sign(SignArgs),

    /// Export a payload key for the online wallet's `import-payload-key`
    ///
    /// The key is derived from the spend key at the given index, so only this signer can produce it. Once the online
    /// wallet has imported it, its payloads are signed with it instead of the shared view key, and
    /// `sign --require-payload-key --payload-key-index <index>` refuses any payload that is not.
    ExportPayloadKey(ExportPayloadKeyArgs),

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
    pub seed_words: String,

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

    /// Deprecated; has no effect.
    #[clap(long, alias = "assume-yes", hide = true)]
    pub yes: bool,

    /// Refuse any payload not signed with a payload key whose index is given with --payload-key-index, including
    /// version 5 payloads and payloads signed with the shared view key
    #[clap(long)]
    pub require_payload_key: bool,

    /// A payload key index to accept with --require-payload-key. Repeat for more than one
    #[clap(long = "payload-key-index", requires = "require_payload_key")]
    pub payload_key_indices: Vec<u64>,
}

#[derive(Debug, Args, Clone)]
pub struct ExportPayloadKeyArgs {
    /// The payload key index. A key is retired by no longer accepting its index when signing
    #[clap(long)]
    pub index: u64,

    /// Write the key to this new file, readable by the owner only, instead of stdout
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// Passphrase to decrypt the keys
    #[clap(long, env = "TARI_PASSPHRASE")]
    pub passphrase: String,
}

impl Cli {
    pub fn execute(self) -> Result<()> {
        self.run(None)
    }

    /// Runs the command. With `out`, `sign` writes to it and signs without a prompt.
    fn run(self, out: Option<&mut dyn Write>) -> Result<()> {
        #[cfg(feature = "test-keystore")]
        let _test_keystore_guard = keystore::set_test_keystore_file(self.test_keystore_file.clone());

        match self.command {
            Commands::Init { method } => match method {
                InitMethod::Keys(args) => init_with_keys(args),
                InitMethod::SeedWords(args) => init_with_seed_words(args),
            },
            Commands::Sign(args) => match out {
                Some(out) => sign_transaction_with_io(args, ConfirmationMode::NonInteractive, out, &mut io::empty()),
                None => sign_transaction(args),
            },
            Commands::ExportPayloadKey(args) => match out {
                Some(out) => export_payload_key(args, out),
                None => export_payload_key(args, &mut io::stdout().lock()),
            },
            Commands::Status => check_status(),
            Commands::Clear => clear_keystore(),
        }
    }
}

pub fn execute_from_args<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args)?.execute()
}

/// Like [`execute_from_args`], but `sign` writes its summary and results to `out` and signs without a prompt.
pub fn execute_from_args_with_output<I, T>(args: I, out: &mut dyn Write) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args)?.run(Some(out))
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
    println!(
        "  Keys have been encrypted and stored in {}.",
        keystore::storage_description()
    );

    Ok(())
}

fn init_with_seed_words(args: InitSeedWordsArgs) -> Result<()> {
    check_already_initialized()?;

    // Get seed words (prompt if not provided)
    let seed_words_str = args.seed_words;

    // Parse seed words
    let seed_words = SeedWords::from_str(&seed_words_str)
        .map_err(|e| OfflineSignerError::InvalidKey(format!("Invalid seed words: {}", e)))?;

    // Convert optional seed passphrase to SafePassword
    let seed_passphrase = args.seed_passphrase.map(SafePassword::from);

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
    println!(
        "  Keys have been derived and encrypted in {}.",
        keystore::storage_description()
    );

    Ok(())
}

/// Shows the operator the transaction and returns it confirmed, before any spend key is used.
fn review_payload(
    summary: PayloadSummary<PrepareOneSidedTransactionForSigningResult>,
    mode: ConfirmationMode,
    out: &mut dyn Write,
    input: &mut dyn BufRead,
) -> Result<ConfirmedPayload<PrepareOneSidedTransactionForSigningResult>> {
    let confirmation = present_payload_summary(&summary, mode, out, input)?;
    summary.confirm(confirmation).map_err(|e| anyhow!("Aborted: {e}"))
}

fn sign_transaction(args: SignArgs) -> Result<()> {
    // Prompt on a terminal; otherwise print the summary and sign
    let mode = if io::stdin().is_terminal() {
        ConfirmationMode::Interactive
    } else {
        ConfirmationMode::NonInteractive
    };
    // When prompting with stdout redirected, show the summary and prompt on stderr so the operator still sees them
    if mode == ConfirmationMode::Interactive && !io::stdout().is_terminal() {
        sign_transaction_with_io(args, mode, &mut io::stderr().lock(), &mut io::stdin().lock())
    } else {
        sign_transaction_with_io(args, mode, &mut io::stdout().lock(), &mut io::stdin().lock())
    }
}

const MAX_FILE_SIZE: u64 = 10_000_000; // 10MB limit

/// Opens the payload file without waiting on a FIFO: on unix the open does not block, and the caller then refuses
/// anything that is not a regular file.
#[cfg(unix)]
fn open_input_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_input_file(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

/// Reads the payload file. Only regular files are read, and never more than the size limit, so a device or pipe in
/// place of the file cannot stall the signer or exhaust memory. All checks run on the opened handle.
fn read_input_file(path: &Path) -> Result<String> {
    let context = || format!("Failed to read input file: {:?}", path);
    #[cfg(not(unix))]
    if !fs::metadata(path).with_context(context)?.is_file() {
        return Err(anyhow!("Input file is not a regular file: {:?}", path));
    }
    let file = open_input_file(path).with_context(context)?;
    let metadata = file.metadata().with_context(context)?;
    if !metadata.is_file() {
        return Err(anyhow!("Input file is not a regular file: {:?}", path));
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err(anyhow!("Input file too large (max {} bytes)", MAX_FILE_SIZE));
    }
    let mut data = String::new();
    file.take(MAX_FILE_SIZE.saturating_add(1))
        .read_to_string(&mut data)
        .with_context(context)?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > MAX_FILE_SIZE {
        return Err(anyhow!("Input file too large (max {} bytes)", MAX_FILE_SIZE));
    }
    Ok(data)
}

fn sign_transaction_with_io(
    args: SignArgs,
    mode: ConfirmationMode,
    out: &mut dyn Write,
    input: &mut dyn BufRead,
) -> Result<()> {
    if args.yes {
        eprintln!(
            "--yes is deprecated and has no effect: without a terminal the signer signs after printing the summary; \
             with a terminal it always asks."
        );
    }

    // Check if initialized
    if !keystore::is_initialized() {
        return Err(anyhow!("Offline signer not initialized. Run 'init' command first."));
    }

    // Get passphrase (prompt if not provided)
    let passphrase = args.passphrase;

    // Parse the network
    let network = Network::from_str(&args.network).map_err(|_| {
        anyhow!(
            "Invalid network '{}'. Valid options: mainnet, nextnet, esmeralda, localnet",
            args.network
        )
    })?;
    // Make the process-wide network (used for signature domains) follow --network
    set_network_if_choice_valid(network).map_err(|e| anyhow!("Invalid network '{}': {}", args.network, e))?;

    // Read and parse the input file
    let data = read_input_file(&args.input_file)?;

    let request = PrepareOneSidedTransactionForSigningResult::from_json(&data)
        .map_err(|e| OfflineSignerError::ParseError(format!("Failed to parse transaction: {}", e)))?;

    let key_manager = open_key_manager(&passphrase)?;
    let policy = if args.require_payload_key {
        PayloadKeyPolicy::RequirePayloadKey {
            accepted_indices: args.payload_key_indices,
        }
    } else {
        PayloadKeyPolicy::AcceptViewKey
    };
    // A payload the signer would refuse is never shown for approval, and "Signed with" in the summary is verified
    verify_offline_payload(&key_manager, &policy, &request)
        .map_err(|e| OfflineSignerError::SigningError(format!("Payload not accepted: {}", e)))?;

    // Show the operator what is about to be signed and get their confirmation, before the spend key signs anything
    let confirmed = review_payload(PayloadSummary::from_one_sided(request), mode, out, input)?;

    // Get consensus constants for the network
    let consensus_manager = ConsensusManager::builder(network).build();
    let consensus_constants = consensus_manager.consensus_constants(0).clone();

    // Sign the transaction
    let result = sign_locked_transaction(&key_manager, consensus_constants, network, confirmed, &policy)
        .map_err(|e| OfflineSignerError::SigningError(format!("Failed to sign transaction: {}", e)))?;

    // Write the signed transaction to the output file
    let json_data = result
        .to_json()
        .map_err(|e| OfflineSignerError::SerializationError(format!("Failed to serialize result: {}", e)))?;

    fs::write(&args.output_file, json_data)
        .with_context(|| format!("Failed to write output file: {:?}", args.output_file))?;

    writeln!(out, "✓ Transaction signed successfully!")?;
    writeln!(out, "  Output written to: {:?}", args.output_file)?;

    Ok(())
}

/// A key manager over the keystore's spend and view keys.
fn open_key_manager(passphrase: &str) -> Result<KeyManager> {
    let (spend_key, view_key) =
        keystore::get_keys(passphrase).map_err(|e| anyhow!("Failed to retrieve keys: {}", e))?;
    let spend_wallet = SpendWallet::new(spend_key, view_key, None);
    Ok(KeyManager::new(WalletType::SpendWallet(spend_wallet))
        .map_err(|e| OfflineSignerError::KeyManagerError(format!("Failed to create key manager: {}", e)))?)
}

fn export_payload_key(args: ExportPayloadKeyArgs, out: &mut dyn Write) -> Result<()> {
    if !keystore::is_initialized() {
        return Err(anyhow!("Offline signer not initialized. Run 'init' command first."));
    }
    let key_manager = open_key_manager(&args.passphrase)?;
    let private_key = key_manager
        .get_private_key(&TariKeyId::PayloadAuth { index: args.index })
        .map_err(|e| OfflineSignerError::KeyManagerError(format!("Failed to derive the payload key: {}", e)))?;
    let export = ExportedPayloadKey {
        index: args.index,
        private_key: private_key.to_hex(),
        public_key: CompressedPublicKey::from_secret_key(&private_key).to_hex(),
    };
    let json = serde_json::to_string_pretty(&export)?;
    match args.output {
        Some(path) => {
            // A new file only: an existing one could already be readable by others
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&path)
                .with_context(|| format!("Failed to create output file: {:?}", path))?;
            writeln!(file, "{json}")?;
            writeln!(out, "✓ Payload key #{} written to {:?}", args.index, path)?;
        },
        None => writeln!(out, "{json}")?,
    }
    Ok(())
}

fn check_status() -> Result<()> {
    if keystore::is_initialized() {
        println!("✓ Offline signer is initialized");
        println!("  Keys are stored in {}", keystore::storage_description());
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

#[cfg(test)]
mod test {
    use tari_common_types::{tari_address::TariAddress, transaction::TxId};
    use tari_transaction_components::{
        MicroMinotari,
        offline_signing::models::{
            OneSidedTransactionInfo,
            PayloadIntegritySignature,
            PaymentRecipient,
            get_latest_version,
        },
        transaction_components::{MemoField, OutputFeatures, memo_field::TxType},
    };

    use super::*;

    fn payload_with_memo(memo: &[u8]) -> PrepareOneSidedTransactionForSigningResult {
        let memo = MemoField::new_open(memo.to_vec(), TxType::PaymentToOther).unwrap();
        PrepareOneSidedTransactionForSigningResult {
            version: get_latest_version(),
            tx_id: TxId::from(1u64),
            info: OneSidedTransactionInfo {
                payment_id: memo.clone(),
                recipients: vec![PaymentRecipient {
                    amount: MicroMinotari(1000),
                    output_features: OutputFeatures::default(),
                    address: TariAddress::default(),
                    payment_id: memo,
                }],
                inputs: vec![],
                outputs: vec![],
                fee: MicroMinotari(0),
                fee_per_gram: MicroMinotari(5),
                sender_address: TariAddress::default(),
            },
            payload_signature: PayloadIntegritySignature {
                signature: Default::default(),
            },
            payload_key_id: None,
            expires_at: u64::MAX,
        }
    }

    #[test]
    fn sign_review_renders_control_characters_escaped() {
        let summary = PayloadSummary::from_one_sided(payload_with_memo(b"\x1b[2J\r\nRecap"));
        let mut out = Vec::new();
        let result = review_payload(summary, ConfirmationMode::NonInteractive, &mut out, &mut io::empty());
        assert!(result.is_ok());
        let written = String::from_utf8(out).unwrap();
        assert!(written.contains("\\u{1b}"), "{written}");
        assert!(!written.contains('\u{1b}'), "{written}");
        let recap = written.find("---- Recap ----").expect("recap");
        assert!(written.find("\\u{1b}").unwrap() < recap, "{written}");
    }

    #[test]
    fn sign_review_declined_is_not_signed() {
        let summary = PayloadSummary::from_one_sided(payload_with_memo(b"memo"));
        let result = review_payload(
            summary,
            ConfirmationMode::Interactive,
            &mut Vec::new(),
            &mut "n\n".as_bytes(),
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Aborted"), "{err}");
    }

    #[test]
    fn input_file_read_refuses_a_directory() {
        let err = read_input_file(&std::env::temp_dir()).unwrap_err().to_string();
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn input_file_read_refuses_a_fifo_without_waiting() {
        let path = std::env::temp_dir().join(format!("offline_signer_fifo_{}", std::process::id()));
        drop(fs::remove_file(&path));
        let status = std::process::Command::new("mkfifo").arg(&path).status().unwrap();
        assert!(status.success());

        // Nothing ever writes to the FIFO, so a blocking open would never return
        let (tx, rx) = std::sync::mpsc::channel();
        let fifo = path.clone();
        std::thread::spawn(move || {
            let result = read_input_file(&fifo).map_err(|e| e.to_string());
            tx.send(result).unwrap();
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(10));
        fs::remove_file(&path).unwrap();
        let err = result.expect("reading a FIFO must not block").unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[test]
    fn input_file_read_refuses_an_oversized_file() {
        let path = std::env::temp_dir().join(format!("offline_signer_oversized_{}.json", std::process::id()));
        fs::write(&path, vec![b' '; 10_000_001]).unwrap();
        let result = read_input_file(&path);
        fs::remove_file(&path).unwrap();
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");
    }

    #[test]
    fn deprecated_yes_flag_still_parses() {
        let cli = Cli::try_parse_from([
            "minotari_offline_signer",
            "sign",
            "-i",
            "a",
            "-o",
            "b",
            "--passphrase",
            "p",
            "--yes",
        ]);
        assert!(cli.is_ok());
    }

    #[test]
    fn payload_key_index_requires_require_payload_key() {
        let args = |extra: &[&str]| {
            let mut args = vec![
                "minotari_offline_signer",
                "sign",
                "-i",
                "a",
                "-o",
                "b",
                "--passphrase",
                "p",
            ];
            args.extend_from_slice(extra);
            Cli::try_parse_from(args)
        };
        assert!(args(&["--payload-key-index", "1"]).is_err());
        let Commands::Sign(sign) = args(&[
            "--require-payload-key",
            "--payload-key-index",
            "1",
            "--payload-key-index",
            "3",
        ])
        .unwrap()
        .command
        else {
            panic!("not a sign command");
        };
        assert!(sign.require_payload_key);
        assert_eq!(sign.payload_key_indices, vec![1, 3]);
    }

    #[cfg(feature = "test-keystore")]
    mod with_keystore {
        use super::*;

        /// A fresh keystore holding a random wallet's keys; returns its file and that wallet.
        fn init_keystore(name: &str) -> (PathBuf, KeyManager) {
            let dir = std::env::temp_dir().join(format!("offline_signer_{name}_{}", std::process::id()));
            drop(fs::remove_dir_all(&dir));
            fs::create_dir_all(&dir).unwrap();
            let keystore = dir.join("keystore.json");
            let wallet = KeyManager::new_random().unwrap();
            let spend_key = wallet.get_private_key(&TariKeyId::SpendKey).unwrap();
            let view_key = wallet.get_private_key(&TariKeyId::ViewKey).unwrap();
            execute_from_args([
                "minotari_offline_signer",
                "--test-keystore-file",
                keystore.to_str().unwrap(),
                "init",
                "keys",
                "--spend-key",
                &spend_key.to_hex(),
                "--view-key",
                &view_key.to_hex(),
                "--passphrase",
                "p",
            ])
            .unwrap();
            (keystore, wallet)
        }

        fn run(keystore: &Path, args: &[&str]) -> Result<String> {
            run_with_output(keystore, args).0
        }

        /// The result, and everything written to the output whether it succeeded or not.
        fn run_with_output(keystore: &Path, args: &[&str]) -> (Result<String>, String) {
            let mut all = vec![
                "minotari_offline_signer",
                "--test-keystore-file",
                keystore.to_str().unwrap(),
            ];
            all.extend_from_slice(args);
            let mut out = Vec::new();
            let result = execute_from_args_with_output(all, &mut out);
            let out = String::from_utf8(out).unwrap();
            (result.map(|()| out.clone()), out)
        }

        #[test]
        fn export_payload_key_writes_a_new_owner_only_file() {
            let (keystore, wallet) = init_keystore("export");
            let file = keystore.with_file_name("payload_key.json");
            let path = file.to_str().unwrap();
            run(&keystore, &[
                "export-payload-key",
                "--index",
                "1",
                "--output",
                path,
                "--passphrase",
                "p",
            ])
            .unwrap();

            let export: ExportedPayloadKey = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
            let expected = wallet.get_private_key(&TariKeyId::PayloadAuth { index: 1 }).unwrap();
            assert_eq!(export.index, 1);
            assert_eq!(export.private_key, expected.to_hex());
            assert_eq!(
                export.public_key,
                CompressedPublicKey::from_secret_key(&expected).to_hex()
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
            }
            // An existing file is never overwritten
            assert!(
                run(&keystore, &[
                    "export-payload-key",
                    "--index",
                    "2",
                    "--output",
                    path,
                    "--passphrase",
                    "p"
                ])
                .is_err()
            );

            // Without --output it goes to stdout
            let out = run(&keystore, &["export-payload-key", "--index", "1", "--passphrase", "p"]).unwrap();
            let printed: ExportedPayloadKey = serde_json::from_str(&out).unwrap();
            assert_eq!(printed.private_key, export.private_key);
            fs::remove_dir_all(keystore.parent().unwrap()).unwrap();
        }

        /// The payload's signature is not valid, so the last case is a payload claiming a key that did not sign it.
        /// Every case is refused before the summary is shown.
        #[test]
        fn sign_applies_the_payload_key_policy_before_the_summary() {
            let (keystore, _) = init_keystore("policy");
            let input = keystore.with_file_name("payload.json");
            let output = keystore.with_file_name("signed.json");
            let sign = |payload: &PrepareOneSidedTransactionForSigningResult, extra: &[&str]| {
                fs::write(&input, payload.to_json().unwrap()).unwrap();
                let mut args = vec![
                    "sign",
                    "-i",
                    input.to_str().unwrap(),
                    "-o",
                    output.to_str().unwrap(),
                    "--passphrase",
                    "p",
                    "-n",
                    "localnet",
                ];
                args.extend_from_slice(extra);
                let (result, out) = run_with_output(&keystore, &args);
                assert!(!out.contains("about to be signed"), "{out}");
                result.unwrap_err().to_string()
            };
            let mut payload = payload_with_memo(b"memo");

            let err = sign(&payload, &["--require-payload-key", "--payload-key-index", "1"]);
            assert!(err.contains("signed with the view key (shared key)"), "{err}");

            payload.payload_key_id = Some(TariKeyId::PayloadAuth { index: 1 });
            let err = sign(&payload, &["--require-payload-key", "--payload-key-index", "2"]);
            assert!(err.contains("payload key #1 is not an accepted index"), "{err}");
            let err = sign(&payload, &["--require-payload-key"]);
            assert!(err.contains("accept-list is empty"), "{err}");
            let err = sign(&payload, &["--require-payload-key", "--payload-key-index", "1"]);
            assert!(err.contains("integrity check failed"), "{err}");
            assert!(!output.exists());
            fs::remove_dir_all(keystore.parent().unwrap()).unwrap();
        }
    }
}
