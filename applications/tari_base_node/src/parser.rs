// Copyright 2019. The Tari Project
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

use std::{string::ToString, sync::Arc, time::Duration};

use futures::future::Either;
use log::*;
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    hint::{Hinter, HistoryHinter},
    line_buffer::LineBuffer,
    Context,
};
use rustyline_derive::{Helper, Highlighter, Validator};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};
use tari_app_utilities::utilities::{UniNodeId, UniPublicKey};
use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_comms::peer_manager::NodeId;
use tari_core::proof_of_work::PowAlgorithm;
use tari_shutdown::Shutdown;
use tari_utilities::{
    hex,
    hex::{from_hex, Hex},
    ByteArray,
};
use tokio::sync::Mutex;

use super::{
    args::{Args, ArgsError, ArgsReason},
    LOG_TARGET,
};
use crate::command_handler::{CommandHandler, Format, StatusOutput};

/// Enum representing commands used by the basenode
#[derive(Clone, Copy, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum BaseNodeCommand {
    Help,
    Version,
    CheckForUpdates,
    Status,
    GetChainMetadata,
    GetDbStats,
    GetPeer,
    ListPeers,
    DialPeer,
    PingPeer,
    ResetOfflinePeers,
    RewindBlockchain,
    BanPeer,
    UnbanPeer,
    UnbanAllPeers,
    ListBannedPeers,
    ListConnections,
    ListHeaders,
    CheckDb,
    PeriodStats,
    HeaderStats,
    BlockTiming,
    CalcTiming,
    ListReorgs,
    DiscoverPeer,
    GetBlock,
    SearchUtxo,
    SearchKernel,
    GetMempoolStats,
    GetMempoolState,
    GetMempoolTx,
    Whoami,
    GetStateInfo,
    GetNetworkStats,
    Quit,
    Exit,
}

/// This is used to parse commands from the user and execute them
#[derive(Helper, Validator, Highlighter)]
pub struct Parser {
    commands: Vec<String>,
    hinter: HistoryHinter,
    command_handler: Arc<Mutex<CommandHandler>>,
}

/// This will go through all instructions and look for potential matches
impl Completer for Parser {
    type Candidate = String;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<String>), ReadlineError> {
        let completions = self
            .commands
            .iter()
            .filter(|cmd| cmd.starts_with(line))
            .cloned()
            .collect();

        Ok((pos, completions))
    }

    fn update(&self, line: &mut LineBuffer, _: usize, elected: &str) {
        line.update(elected, elected.len());
    }
}

/// This allows us to make hints based on historic inputs
impl Hinter for Parser {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Parser {
    /// creates a new parser struct
    pub fn new(command_handler: Arc<Mutex<CommandHandler>>) -> Self {
        Parser {
            commands: BaseNodeCommand::iter().map(|x| x.to_string()).collect(),
            hinter: HistoryHinter {},
            command_handler,
        }
    }

    /// This will return the list of commands from the parser
    pub fn get_commands(&self) -> Vec<String> {
        self.commands.clone()
    }

    /// This will parse the provided command and execute the task
    pub async fn handle_command(&mut self, command_str: &str, shutdown: &mut Shutdown) {
        if command_str.trim().is_empty() {
            return;
        }

        let mut typed_args = Args::split(command_str);
        let command = typed_args.take_next("command");
        let args = command_str.split_whitespace();
        match command {
            Ok(command) => {
                let res = self.process_command(command, args, typed_args, shutdown).await;
                if let Err(err) = res {
                    println!("Command Error: {}", err);
                    self.print_help(command);
                }
            },
            Err(_) => {
                println!("{} is not a valid command, please enter a valid command", command_str);
                println!("Enter help or press tab for available commands");
            },
        }
    }

    pub fn get_command_handler(&self) -> Arc<Mutex<CommandHandler>> {
        self.command_handler.clone()
    }

    /// Function to process commands
    async fn process_command<'a, I: Iterator<Item = &'a str>>(
        &mut self,
        command: BaseNodeCommand,
        args: I,
        mut typed_args: Args<'a>,
        shutdown: &mut Shutdown,
    ) -> Result<(), ArgsError> {
        use BaseNodeCommand::*;
        match command {
            Help => {
                let command = typed_args.take_next("help-command")?;
                self.print_help(command);
            },
            Status => {
                self.command_handler.lock().await.status(StatusOutput::Full);
            },
            GetStateInfo => {
                self.command_handler.lock().await.state_info();
            },
            Version => {
                self.command_handler.lock().await.print_version();
            },
            CheckForUpdates => {
                self.command_handler.lock().await.check_for_updates();
            },
            GetChainMetadata => {
                self.command_handler.lock().await.get_chain_meta();
            },
            GetDbStats => {
                self.command_handler.lock().await.get_blockchain_db_stats();
            },
            DialPeer => {
                self.process_dial_peer(typed_args).await?;
            },
            PingPeer => {
                self.process_ping_peer(typed_args).await?;
            },
            DiscoverPeer => {
                self.process_discover_peer(typed_args).await?;
            },
            GetPeer => {
                self.process_get_peer(typed_args).await?;
            },
            ListPeers => {
                self.process_list_peers(typed_args).await;
            },
            ResetOfflinePeers => {
                self.command_handler.lock().await.reset_offline_peers();
            },
            RewindBlockchain => {
                self.process_rewind_blockchain(typed_args).await?;
            },
            CheckDb => {
                self.command_handler.lock().await.check_db();
            },
            PeriodStats => {
                self.process_period_stats(typed_args).await?;
            },
            HeaderStats => {
                self.process_header_stats(typed_args).await?;
            },
            BanPeer => {
                self.process_ban_peer(typed_args, true).await?;
            },
            UnbanPeer => {
                self.process_ban_peer(typed_args, false).await?;
            },
            UnbanAllPeers => {
                self.command_handler.lock().await.unban_all_peers();
            },
            ListBannedPeers => {
                self.command_handler.lock().await.list_banned_peers();
            },
            ListConnections => {
                self.command_handler.lock().await.list_connections();
            },
            ListHeaders => {
                self.process_list_headers(typed_args).await?;
            },
            BlockTiming | CalcTiming => {
                self.process_block_timing(typed_args).await?;
            },
            ListReorgs => {
                self.process_list_reorgs().await;
            },
            GetBlock => {
                self.process_get_block(args).await;
            },
            SearchUtxo => {
                self.process_search_utxo(typed_args).await?;
            },
            SearchKernel => {
                self.process_search_kernel(args).await;
            },
            GetMempoolStats => {
                self.command_handler.lock().await.get_mempool_stats();
            },
            GetMempoolState => {
                self.command_handler.lock().await.get_mempool_state(None);
            },
            GetMempoolTx => {
                self.get_mempool_state_tx(typed_args).await?;
            },
            Whoami => {
                self.command_handler.lock().await.whoami();
            },
            GetNetworkStats => {
                self.command_handler.lock().await.get_network_stats();
            },
            Exit | Quit => {
                println!("Shutting down...");
                info!(
                    target: LOG_TARGET,
                    "Termination signal received from user. Shutting node down."
                );
                let _ = shutdown.trigger();
            },
        }
        // TODO: Remove it (use expressions above)
        Ok(())
    }

    /// Displays the commands or context specific help for a given command
    fn print_help(&self, command: BaseNodeCommand) {
        use BaseNodeCommand::*;
        match command {
            Help => {
                println!("Available commands are: ");
                let joined = self.commands.join(", ");
                println!("{}", joined);
            },
            Status => {
                println!("Prints out the status of this node");
            },
            GetStateInfo => {
                println!("Prints out the status of the base node state machine");
            },
            Version => {
                println!("Gets the current application version");
            },
            CheckForUpdates => {
                println!("Checks for software updates if auto update is enabled");
            },
            GetChainMetadata => {
                println!("Gets your base node chain meta data");
            },
            GetDbStats => {
                println!("Gets your base node database stats");
            },
            DialPeer => {
                println!("Attempt to connect to a known peer");
                println!("dial-peer [hex public key or emoji id]");
            },
            PingPeer => {
                println!("Send a ping to a known peer and wait for a pong reply");
                println!("ping-peer [hex public key or emoji id]");
            },
            DiscoverPeer => {
                println!("Attempt to discover a peer on the Tari network");
                println!("discover-peer [hex public key or emoji id]");
            },
            GetPeer => {
                println!("Get all available info about peer");
                println!("Usage: get-peer [Partial NodeId | PublicKey | EmojiId]");
            },
            ListPeers => {
                println!("Lists the peers that this node knows about");
            },
            ResetOfflinePeers => {
                println!("Clear offline flag from all peers");
            },
            RewindBlockchain => {
                println!("Rewinds the blockchain to the given height.");
                println!("Usage: {} [new_height]", command);
                println!("new_height must be less than the current height.");
            },
            BanPeer => {
                println!("Bans a peer");
                println!(
                    "ban-peer/unban-peer [hex public key or emoji id] (length of time to ban the peer for in seconds)"
                );
            },
            UnbanPeer => {
                println!("Removes a peer ban");
            },
            UnbanAllPeers => {
                println!("Unbans all peers");
            },
            ListBannedPeers => {
                println!("Lists peers that have been banned by the node or wallet");
            },
            CheckDb => {
                println!("Checks the blockchain database for missing blocks and headers");
            },
            HeaderStats => {
                println!(
                    "Prints out certain stats to of the block chain in csv format for easy copy, use as follows: "
                );
                println!("header-stats [start height] [end height] (dump_file) (filter:monero|sha3)");
                println!("e.g.");
                println!("header-stats 0 1000");
                println!("header-stats 0 1000 sample2.csv");
                println!("header-stats 0 1000 monero-sample.csv monero");
            },
            PeriodStats => {
                println!(
                    "Prints out certain aggregated stats to of the block chain in csv format for easy copy, use as \
                     follows: "
                );
                println!(
                    "Period-stats [start time in unix timestamp] [end time in unix timestamp] [interval period time \
                     in unix timestamp]"
                );
            },
            ListConnections => {
                println!("Lists the peer connections currently held by this node");
            },
            ListHeaders => {
                println!("List the amount of headers, can be called in the following two ways: ");
                println!("list-headers [first header height] [last header height]");
                println!("list-headers [number of headers starting from the chain tip back]");
            },
            BlockTiming | CalcTiming => {
                println!("Calculates the maximum, minimum, and average time taken to mine a given range of blocks.");
                println!("block-timing [start height] [end height]");
                println!("block-timing [number of blocks from chain tip]");
            },
            ListReorgs => {
                println!("List tracked reorgs.");
                println!(
                    "This feature must be enabled by setting `track_reorgs = true` in the [base_node] section of your \
                     config."
                );
            },
            GetBlock => {
                println!("Display a block by height or hash:");
                println!("get-block [height or hash of the block] [format]");
                println!(
                    "[height or hash of the block] The height or hash of the block to fetch from the main chain. The \
                     genesis block has height zero."
                );
                println!(
                    "[format] Optional. Supported options are 'json' and 'text'. 'text' is the default if omitted."
                );
            },
            SearchUtxo => {
                println!(
                    "This will search the main chain for the utxo. If the utxo is found, it will print out the block \
                     it was found in."
                );
                println!("search-utxo [hex of commitment of the utxo]");
            },
            SearchKernel => {
                println!(
                    "This will search the main chain for the kernel. If the kernel is found, it will print out the \
                     block it was found in."
                );
                println!("This searches for the kernel via the excess signature");
                println!("search-kernel [hex of nonce] [Hex of signature]");
            },
            GetMempoolStats => {
                println!("Retrieves your mempools stats");
            },
            GetMempoolState => {
                println!("Retrieves your mempools state");
            },
            GetMempoolTx => {
                println!("Filters and retrieves details about transactions from the mempool's state");
            },
            Whoami => {
                println!(
                    "Display identity information about this node, including: public key, node ID and the public \
                     address"
                );
            },
            GetNetworkStats => {
                println!("Displays network stats");
            },
            Exit | Quit => {
                println!("Exits the base node");
            },
        }
    }

    /// Function to process the get-block command
    async fn process_get_block<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let height_or_hash = match args.next() {
            Some(s) => s
                .parse::<u64>()
                .ok()
                .map(Either::Left)
                .or_else(|| from_hex(s).ok().map(Either::Right)),
            None => {
                self.print_help(BaseNodeCommand::GetBlock);
                return;
            },
        };

        let format = match args.next() {
            Some(v) if v.to_ascii_lowercase() == "json" => Format::Json,
            Some(v) if v.to_ascii_lowercase() == "text" => Format::Text,
            None => Format::Text,
            Some(_) => {
                println!("Unrecognized format specifier");
                self.print_help(BaseNodeCommand::GetBlock);
                return;
            },
        };

        match height_or_hash {
            Some(Either::Left(height)) => self.command_handler.lock().await.get_block(height, format),
            Some(Either::Right(hash)) => self.command_handler.lock().await.get_block_by_hash(hash, format),
            None => {
                println!("Invalid block height or hash provided. Height must be an integer.");
                self.print_help(BaseNodeCommand::GetBlock);
            },
        };
    }

    /// Function to process the search utxo command
    async fn process_search_utxo<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let hex: String = args.take_next("hex")?;
        let commitment = Commitment::from_hex(&hex)
            .map_err(|err| ArgsError::new("hex", format!("Invalid commitment provided: {}", err)))?;
        self.command_handler.lock().await.search_utxo(commitment);
        Ok(())
    }

    /// Function to process the search kernel command
    async fn process_search_kernel<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        // let command_arg = args.take(4).collect::<Vec<&str>>();
        let hex = args.next();
        if hex.is_none() {
            self.print_help(BaseNodeCommand::SearchKernel);
            return;
        }
        let public_nonce = match PublicKey::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid public nonce provided.");
                self.print_help(BaseNodeCommand::SearchKernel);
                return;
            },
        };

        let hex = args.next();
        if hex.is_none() {
            self.print_help(BaseNodeCommand::SearchKernel);
            return;
        }
        let signature = match PrivateKey::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid signature provided.");
                self.print_help(BaseNodeCommand::SearchKernel);
                return;
            },
        };
        let kernel_sig = Signature::new(public_nonce, signature);

        self.command_handler.lock().await.search_kernel(kernel_sig)
    }

    async fn get_mempool_state_tx<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let filter = args.take_next("filter").ok();
        self.command_handler.lock().await.get_mempool_state(filter);
        Ok(())
    }

    /// Function to process the discover-peer command
    async fn process_discover_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let key: UniPublicKey = args.take_next("id")?;
        self.command_handler.lock().await.discover_peer(Box::new(key.into()));
        Ok(())
    }

    async fn process_get_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let original_str: String = args
            .try_take_next("node_id")?
            .ok_or_else(|| ArgsError::new("node_id", ArgsReason::Required))?;
        let node_id: Result<UniNodeId, _> = args.take_next("node_id");
        let partial;
        match node_id {
            Ok(n) => {
                partial = NodeId::from(n).to_vec();
            },
            Err(_) => {
                let s = &original_str;
                // TODO: No idea why we did that
                let bytes = hex::from_hex(&s[..s.len() - (s.len() % 2)]).unwrap_or_default();
                partial = bytes;
            },
        }
        self.command_handler.lock().await.get_peer(partial, original_str);
        Ok(())
    }

    /// Function to process the list-peers command
    async fn process_list_peers<'a>(&mut self, mut args: Args<'a>) {
        let filter = args.take_next("filter").ok();
        self.command_handler.lock().await.list_peers(filter)
    }

    /// Function to process the dial-peer command
    async fn process_dial_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let dest_node_id: UniNodeId = args.take_next("node-id")?;
        self.command_handler.lock().await.dial_peer(dest_node_id.into());
        Ok(())
    }

    /// Function to process the dial-peer command
    async fn process_ping_peer<'a>(&mut self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let dest_node_id: UniNodeId = args.take_next("node-id")?;
        self.command_handler.lock().await.ping_peer(dest_node_id.into());
        Ok(())
    }

    /// Function to process the ban-peer command
    async fn process_ban_peer<'a>(&mut self, mut args: Args<'a>, must_ban: bool) -> Result<(), ArgsError> {
        let node_id: UniNodeId = args.take_next("node-id")?;
        let secs = args.try_take_next("length")?.unwrap_or(std::u64::MAX);
        let duration = Duration::from_secs(secs);
        self.command_handler
            .lock()
            .await
            .ban_peer(node_id.into(), duration, must_ban);
        Ok(())
    }

    /// Function to process the list-headers command
    async fn process_list_headers<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let start = args.take_next("start")?;
        let end = args.try_take_next("end")?;
        self.command_handler.lock().await.list_headers(start, end);
        Ok(())
    }

    /// Function to process the calc-timing command
    async fn process_block_timing<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let start = args.take_next("start")?;
        let end = args.try_take_next("end")?;
        if end.is_none() && start < 2 {
            Err(ArgsError::new("start", "Number of headers must be at least 2."))
        } else {
            self.command_handler.lock().await.block_timing(start, end);
            Ok(())
        }
    }

    async fn process_period_stats<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let period_end = args.take_next("period_end")?;
        let period_ticker_end = args.take_next("period_ticker_end")?;
        let period = args.take_next("period")?;
        self.command_handler
            .lock()
            .await
            .period_stats(period_end, period_ticker_end, period);
        Ok(())
    }

    async fn process_header_stats<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let start_height: u64 = args.take_next("start_height")?;
        let end_height: u64 = args.take_next("end_height")?;
        let filename: String = args
            .try_take_next("filename")?
            .unwrap_or_else(|| "header-data.csv".into());

        // TODO: Replace that with a struct that impl `FromStr`
        let algo_arg: Option<String> = args.try_take_next("algo")?;
        let algo_str: Option<&str> = algo_arg.as_ref().map(String::as_ref);
        let algo = match algo_str {
            Some("monero") => Some(PowAlgorithm::Monero),
            Some("sha") | Some("sha3") => Some(PowAlgorithm::Sha3),
            None | Some("all") => None,
            _ => return Err(ArgsError::new("algo", "Invalid pow algo")),
        };

        self.command_handler
            .lock()
            .await
            .save_header_stats(start_height, end_height, filename, algo);
        Ok(())
    }

    async fn process_rewind_blockchain<'a>(&self, mut args: Args<'a>) -> Result<(), ArgsError> {
        let new_height = args.take_next("new_height")?;
        self.command_handler.lock().await.rewind_blockchain(new_height);
        Ok(())
    }

    async fn process_list_reorgs(&self) {
        self.command_handler.lock().await.list_reorgs();
    }
}
