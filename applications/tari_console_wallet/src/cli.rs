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

use std::{path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use tari_app_utilities::{common_cli_args::CommonCliArgs, utilities::UniPublicKey};
use tari_comms::multiaddr::Multiaddr;
use tari_core::transactions::{tari_amount, tari_amount::MicroTari};
use tari_utilities::{
    hex::{Hex, HexError},
    SafePassword,
};

const DEFAULT_NETWORK: &str = "dibbler";

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// Enable tracing
    #[clap(long, aliases = &["tracing", "enable-tracing"])]
    pub tracing_enabled: bool,
    /// Supply the password for the console wallet. It's very bad security practice to provide the password on the
    /// command line, since it's visible using `ps ax` from anywhere on the system, so always use the env var where
    /// possible.
    #[clap(long, env = "TARI_WALLET_PASSWORD", hide_env_values = true)]
    pub password: Option<SafePassword>,
    /// Change the password for the console wallet
    #[clap(long, alias = "update-password")]
    pub change_password: bool,
    /// Force wallet recovery
    #[clap(long, alias = "recover")]
    pub recovery: bool,
    /// Supply the optional wallet seed words for recovery on the command line
    #[clap(long, alias = "seed-words")]
    pub seed_words: Option<String>,
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
    /// Supply a network (overrides existing configuration)
    #[clap(long, default_value = DEFAULT_NETWORK, env = "TARI_NETWORK")]
    pub network: String,
    #[clap(subcommand)]
    pub command2: Option<CliCommands>,
}

impl Cli {
    pub fn config_property_overrides(&self) -> Vec<(String, String)> {
        let mut overrides = self.common.config_property_overrides();
        overrides.push(("wallet.override_from".to_string(), self.network.clone()));
        overrides.push(("p2p.seeds.override_from".to_string(), self.network.clone()));
        overrides
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand, Clone)]
pub enum CliCommands {
    GetBalance,
    SendTari(SendTariArgs),
    SendOneSided(SendTariArgs),
    SendOneSidedToStealthAddress(SendTariArgs),
    MakeItRain(MakeItRainArgs),
    CoinSplit(CoinSplitArgs),
    DiscoverPeer(DiscoverPeerArgs),
    Whois(WhoisArgs),
    ExportUtxos(ExportUtxosArgs),
    ExportSpentUtxos(ExportUtxosArgs),
    CountUtxos,
    SetBaseNode(SetBaseNodeArgs),
    SetCustomBaseNode(SetBaseNodeArgs),
    ClearCustomBaseNode,
    InitShaAtomicSwap(SendTariArgs),
    FinaliseShaAtomicSwap(FinaliseShaAtomicSwapArgs),
    ClaimShaAtomicSwapRefund(ClaimShaAtomicSwapRefundArgs),
    RevalidateWalletDb,
    Contract(ContractCommand),
}

#[derive(Debug, Args, Clone)]
pub struct DiscoverPeerArgs {
    pub dest_public_key: UniPublicKey,
}

#[derive(Debug, Args, Clone)]
pub struct SendTariArgs {
    pub amount: MicroTari,
    pub destination: UniPublicKey,
    #[clap(short, long, default_value = "<No message>")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct MakeItRainArgs {
    pub destination: UniPublicKey,
    #[clap(short, long, alias="amount", default_value_t = tari_amount::T)]
    pub start_amount: MicroTari,
    #[clap(short, long, alias = "tps", default_value_t = 25)]
    pub transactions_per_second: u32,
    #[clap(short, long, parse(try_from_str = parse_duration), default_value="60")]
    pub duration: Duration,
    #[clap(long, default_value_t=tari_amount::T)]
    pub increase_amount: MicroTari,
    #[clap(long)]
    pub start_time: Option<DateTime<Utc>>,
    #[clap(short, long)]
    pub one_sided: bool,
    #[clap(short, long, default_value = "Make it rain")]
    pub message: String,
}

fn parse_duration(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(std::time::Duration::from_secs(seconds))
}

#[derive(Debug, Args, Clone)]
pub struct CoinSplitArgs {
    pub amount_per_split: MicroTari,
    pub num_splits: usize,
    #[clap(short, long, default_value = "1")]
    pub fee_per_gram: MicroTari,
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
}

#[derive(Debug, Args, Clone)]
pub struct SetBaseNodeArgs {
    pub public_key: UniPublicKey,
    pub address: Multiaddr,
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

fn parse_hex(s: &str) -> Result<Vec<u8>, HexError> {
    Vec::<u8>::from_hex(s)
}

#[derive(Debug, Args, Clone)]
pub struct ClaimShaAtomicSwapRefundArgs {
    #[clap(short, long, parse(try_from_str = parse_hex), required = true)]
    pub output_hash: Vec<Vec<u8>>,
    #[clap(short, long, default_value = "Claimed HTLC atomic swap refund")]
    pub message: String,
}

#[derive(Debug, Args, Clone)]
pub struct ContractCommand {
    #[clap(subcommand)]
    pub subcommand: ContractSubcommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ContractSubcommand {
    /// Generates a new contract definition JSON spec file that can be edited and passed to other contract definition
    /// commands.
    InitDefinition(InitDefinitionArgs),

    /// A generator for constitution files that can be edited and passed to other contract commands
    InitConstitution(InitConstitutionArgs),

    /// A generator for update proposal files that can be edited and passed to other contract commands
    InitUpdateProposal(InitUpdateProposalArgs),

    /// A generator for amendment files that can be edited and passed to other contract commands
    InitAmendment(InitAmendmentArgs),

    /// Creates and publishes a contract definition UTXO from the JSON spec file.
    PublishDefinition(PublishFileArgs),

    /// Creates and publishes a contract definition UTXO from the JSON spec file.
    PublishConstitution(PublishFileArgs),

    /// Creates and publishes a contract update proposal UTXO from the JSON spec file.
    PublishUpdateProposal(PublishFileArgs),

    /// Creates and publishes a contract amendment UTXO from the JSON spec file.
    PublishAmendment(PublishFileArgs),
}

#[derive(Debug, Args, Clone)]
pub struct InitDefinitionArgs {
    /// The destination path of the contract definition to create
    pub dest_path: PathBuf,
    /// Force overwrite the destination file if it already exists
    #[clap(short = 'f', long)]
    pub force: bool,
    #[clap(long, alias = "name")]
    pub contract_name: Option<String>,
    #[clap(long, alias = "issuer")]
    pub contract_issuer: Option<String>,
    #[clap(long, alias = "runtime")]
    pub runtime: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct InitConstitutionArgs {
    /// The destination path of the contract definition to create
    pub dest_path: PathBuf,
    /// Force overwrite the destination file if it already exists
    #[clap(short = 'f', long)]
    pub force: bool,
    #[clap(long, alias = "id")]
    pub contract_id: Option<String>,
    #[clap(long, alias = "committee")]
    pub validator_committee: Option<Vec<String>>,
    #[clap(long, alias = "acceptance_period")]
    pub acceptance_period_expiry: Option<String>,
    #[clap(long, alias = "quorum_required")]
    pub minimum_quorum_required: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct InitUpdateProposalArgs {
    /// The destination path of the contract definition to create
    pub dest_path: PathBuf,
    /// Force overwrite the destination file if it already exists
    #[clap(short = 'f', long)]
    pub force: bool,
    #[clap(long, alias = "id")]
    pub contract_id: Option<String>,
    #[clap(long, alias = "proposal_id")]
    pub proposal_id: Option<String>,
    #[clap(long, alias = "committee")]
    pub validator_committee: Option<Vec<String>>,
    #[clap(long, alias = "acceptance_period")]
    pub acceptance_period_expiry: Option<String>,
    #[clap(long, alias = "quorum_required")]
    pub minimum_quorum_required: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct InitAmendmentArgs {
    /// The destination path of the contract amendment to create
    pub dest_path: PathBuf,

    /// Force overwrite the destination file if it already exists
    #[clap(short = 'f', long)]
    pub force: bool,

    /// The source file path of the update proposal to amend
    #[clap(short = 'p', long)]
    pub proposal_file_path: PathBuf,

    #[clap(long, alias = "activation_window")]
    pub activation_window: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct PublishFileArgs {
    pub file_path: PathBuf,
}

#[derive(Debug, Args, Clone)]
pub struct PublishUpdateProposalArgs {
    pub file_path: PathBuf,
}
