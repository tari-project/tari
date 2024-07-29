//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt::{Debug, Display, Formatter},
    path::PathBuf,
    time::Duration,
};

use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use minotari_app_utilities::{common_cli_args::CommonCliArgs, utilities::UniPublicKey};
use tari_common::configuration::{ConfigOverrideProvider, Network};
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_core::transactions::{tari_amount, tari_amount::MicroMinotari};
use tari_key_manager::SeedWords;
use tari_utilities::{
    hex::{Hex, HexError},
    SafePassword,
};
use thiserror::Error;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// Supply the password for the console wallet. It's very bad security practice to provide the password on the
    /// command line, since it's visible using `ps ax` from anywhere on the system, so always use the env var where
    /// possible.
    #[clap(long, env = "MINOTARI_WALLET_PASSWORD", hide_env_values = true)]
    pub password: Option<SafePassword>,
    /// Change the password for the console wallet and exit
    #[clap(long, alias = "update-password")]
    pub change_password: bool,
    /// Force wallet recovery
    #[clap(long, alias = "recover")]
    pub recovery: bool,
    /// Supply the optional wallet seed words for recovery on the command line. They should be in one string space
    /// separated. e.g. --seed-words "seed1 seed2 ..."
    #[clap(long, alias = "seed-words")]
    pub seed_words: Option<SeedWords>,
    /// Supply the optional file name to save the wallet seed words into
    #[clap(long, aliases = &["seed_words_file_name", "seed-words-file"], parse(from_os_str))]
    pub seed_words_file_name: Option<PathBuf>,
    /// Run in non-interactive mode, with no UI.
    #[clap(short, long, alias = "non-interactive")]
    pub non_interactive_mode: bool,
    /// Path to input file of commands
    #[clap(short, long, aliases = &["input", "script"], parse(from_os_str))]
    pub input_file: Option<PathBuf>,
    /// Single input command
    #[clap(long)]
    pub command: Option<String>,
    /// Wallet notify script
    #[clap(long, alias = "notify")]
    pub wallet_notify: Option<PathBuf>,
    /// Automatically exit wallet command/script mode when done
    #[clap(long, alias = "auto-exit")]
    pub command_mode_auto_exit: bool,
    #[clap(long, env = "MINOTARI_WALLET_ENABLE_GRPC", alias = "enable-grpc")]
    pub grpc_enabled: bool,
    #[clap(long, env = "MINOTARI_WALLET_GRPC_ADDRESS")]
    pub grpc_address: Option<String>,
    #[clap(subcommand)]
    pub command2: Option<CliCommands>,
    #[clap(long, alias = "profile")]
    pub profile_with_tokio_console: bool,
}

impl ConfigOverrideProvider for Cli {
    fn get_config_property_overrides(&self, network: &mut Network) -> Vec<(String, String)> {
        let mut overrides = self.common.get_config_property_overrides(network);
        *network = self.common.network.unwrap_or(*network);
        overrides.push(("wallet.network".to_string(), network.to_string()));
        overrides.push(("wallet.override_from".to_string(), network.to_string()));
        overrides.push(("p2p.seeds.override_from".to_string(), network.to_string()));
        // Either of these configs enable grpc
        if let Some(ref addr) = self.grpc_address {
            overrides.push(("wallet.grpc_enabled".to_string(), "true".to_string()));
            overrides.push(("wallet.grpc_address".to_string(), addr.clone()));
        } else if self.grpc_enabled {
            overrides.push(("wallet.grpc_enabled".to_string(), "true".to_string()));
        } else {
            // GRPC is disabled
        }
        overrides
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand, Clone)]
pub enum CliCommands {
    GetBalance,
    SendMinotari(SendMinotariArgs),
    BurnMinotari(BurnMinotariArgs),
    PreMineGenerateSessionInfo(PreMineGenerateSessionInfoArgs),
    PreMineCreatePartyDetails(PreMineCreatePartyDetailsArgs),
    PreMineEncumberAggregateUtxo(PreMineEncumberAggregateUtxoArgs),
    PreMineCreateInputOutputSigs(PreMineCreateInputOutputSigArgs),
    PreMineSpendAggregateUtxo(PreMineSpendAggregateUtxoArgs),
    PreMineSpendBackupUtxo(PreMineSpendBackupUtxoArgs),
    SendOneSidedToStealthAddress(SendMinotariArgs),
    MakeItRain(MakeItRainArgs),
    CoinSplit(CoinSplitArgs),
    DiscoverPeer(DiscoverPeerArgs),
    Whois(WhoisArgs),
    ExportUtxos(ExportUtxosArgs),
    ExportTx(ExportTxArgs),
    ImportTx(ImportTxArgs),
    ExportSpentUtxos(ExportUtxosArgs),
    CountUtxos,
    SetBaseNode(SetBaseNodeArgs),
    SetCustomBaseNode(SetBaseNodeArgs),
    ClearCustomBaseNode,
    InitShaAtomicSwap(SendMinotariArgs),
    FinaliseShaAtomicSwap(FinaliseShaAtomicSwapArgs),
    ClaimShaAtomicSwapRefund(ClaimShaAtomicSwapRefundArgs),
    RevalidateWalletDb,
    RegisterValidatorNode(RegisterValidatorNodeArgs),
    CreateTlsCerts,
    Sync(SyncArgs)
}

#[derive(Debug, Args, Clone)]
pub struct DiscoverPeerArgs {
    pub dest_public_key: UniPublicKey,
}

#[derive(Debug, Args, Clone)]
pub struct SendMinotariArgs {
    pub amount: MicroMinotari,
    pub destination: TariAddress,
    #[clap(short, long, default_value = "<No message>")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct BurnMinotariArgs {
    pub amount: MicroMinotari,
    #[clap(short, long, default_value = "Burn funds")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineGenerateSessionInfoArgs {
    #[clap(long)]
    pub fee_per_gram: MicroMinotari,
    #[clap(long)]
    pub commitment: String,
    #[clap(long)]
    pub output_hash: String,
    #[clap(long)]
    pub recipient_address: TariAddress,
    #[clap(long)]
    pub verify_unspent_outputs: bool,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineCreatePartyDetailsArgs {
    #[clap(long)]
    pub input_file: PathBuf,
    #[clap(long)]
    pub alias: String,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineEncumberAggregateUtxoArgs {
    #[clap(long)]
    pub session_id: String,
    #[clap(long)]
    pub input_file_names: Vec<String>,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineCreateInputOutputSigArgs {
    #[clap(long)]
    pub session_id: String,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineSpendAggregateUtxoArgs {
    #[clap(long)]
    pub session_id: String,
    #[clap(long)]
    pub input_file_names: Vec<String>,
}

#[derive(Debug, Args, Clone)]
pub struct PreMineSpendBackupUtxoArgs {
    #[clap(long)]
    pub fee_per_gram: MicroMinotari,
    #[clap(long)]
    pub commitment: String,
    #[clap(long)]
    pub output_hash: String,
    #[clap(long)]
    pub recipient_address: TariAddress,
}

#[derive(Debug, Args, Clone)]
pub struct MakeItRainArgs {
    pub destination: TariAddress,
    #[clap(short, long, alias="amount", default_value_t = tari_amount::T)]
    pub start_amount: MicroMinotari,
    #[clap(short, long, alias = "tps", default_value_t = 25.0)]
    pub transactions_per_second: f64,
    #[clap(short, long, parse(try_from_str = parse_duration), default_value="60")]
    pub duration: Duration,
    #[clap(long, default_value_t=tari_amount::T)]
    pub increase_amount: MicroMinotari,
    #[clap(long, parse(try_from_str=parse_start_time))]
    pub start_time: Option<DateTime<Utc>>,
    #[clap(long, alias = "stealth-one-sided")]
    pub one_sided: bool,
    #[clap(short, long)]
    pub burn_tari: bool,
    #[clap(short, long, default_value = "Make it rain")]
    pub message: String,
}

impl MakeItRainArgs {
    pub fn transaction_type(&self) -> MakeItRainTransactionType {
        if self.one_sided {
            MakeItRainTransactionType::StealthOneSided
        } else if self.burn_tari {
            MakeItRainTransactionType::BurnTari
        } else {
            MakeItRainTransactionType::Interactive
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MakeItRainTransactionType {
    Interactive,
    StealthOneSided,
    BurnTari,
}

impl Display for MakeItRainTransactionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

fn parse_start_time(arg: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    let mut start_time = Utc::now();
    if !arg.is_empty() && arg.to_uppercase() != "NOW" {
        start_time = arg.parse()?;
    }
    Ok(start_time)
}

fn parse_duration(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(std::time::Duration::from_secs(seconds))
}

#[derive(Debug, Args, Clone)]
pub struct CoinSplitArgs {
    pub amount_per_split: MicroMinotari,
    pub num_splits: usize,
    #[clap(short, long, default_value = "1")]
    pub fee_per_gram: MicroMinotari,
    #[clap(short, long, default_value = "Coin split")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct WhoisArgs {
    pub public_key: UniPublicKey,
}

#[derive(Debug, Args, Clone)]
pub struct ExportUtxosArgs {
    #[clap(short, long)]
    pub output_file: Option<PathBuf>,
    pub with_private_keys: bool,
}

#[derive(Debug, Args, Clone)]
pub struct ExportTxArgs {
    pub tx_id: u64,
    #[clap(short, long)]
    pub output_file: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
pub struct ImportTxArgs {
    #[clap(short, long)]
    pub input_file: PathBuf,
}

#[derive(Debug, Args, Clone)]
pub struct SetBaseNodeArgs {
    pub public_key: UniPublicKey,
    pub address: Multiaddr,
}

#[derive(Debug, Error, PartialEq)]
enum CliParseError {
    #[error("Could not convert into hex: `{0}`")]
    HexError(String),
}

impl From<HexError> for CliParseError {
    fn from(e: HexError) -> Self {
        CliParseError::HexError(e.to_string())
    }
}

#[derive(Debug, Args, Clone)]
pub struct FinaliseShaAtomicSwapArgs {
    #[clap(short, long, parse(try_from_str = parse_hex), required=true )]
    pub output_hash: Vec<Vec<u8>>,
    #[clap(short, long)]
    pub pre_image: UniPublicKey,
    #[clap(short, long, default_value = "Claimed HTLC atomic swap")]
    pub message: String,
}

fn parse_hex(s: &str) -> Result<Vec<u8>, CliParseError> {
    Vec::<u8>::from_hex(s).map_err(|e| CliParseError::HexError(format!("{}", e)))
}

#[derive(Debug, Args, Clone)]
pub struct ClaimShaAtomicSwapRefundArgs {
    #[clap(short, long, parse(try_from_str = parse_hex), required = true)]
    pub output_hash: Vec<Vec<u8>>,
    #[clap(short, long, default_value = "Claimed HTLC atomic swap refund")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct RegisterValidatorNodeArgs {
    pub amount: MicroMinotari,
    pub validator_node_public_key: UniPublicKey,
    pub validator_node_public_nonce: UniPublicKey,
    pub validator_node_signature: Vec<u8>,
    #[clap(short, long, default_value = "Registering VN")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct SyncArgs {
    #[clap(short, long, default_value = "0")]
    pub sync_to_height: u64,
}
