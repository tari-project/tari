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

use super::LOG_TARGET;
use crate::command_handler::{delimit_command_string, CommandHandler, Format};
use log::*;
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    hint::{Hinter, HistoryHinter},
    line_buffer::LineBuffer,
    Context,
};
use rustyline_derive::{Helper, Highlighter, Validator};
use std::{str::FromStr, string::ToString, sync::Arc, time::Duration};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};
use tari_app_utilities::utilities::{
    either_to_node_id,
    parse_emoji_id_or_public_key,
    parse_emoji_id_or_public_key_or_node_id,
};
use tari_core::{
    tari_utilities::hex::Hex,
    transactions::types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_shutdown::Shutdown;

/// Enum representing commands used by the basenode
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum BaseNodeCommand {
    Help,
    Version,
    Status,
    GetChainMetadata,
    GetPeer,
    ListPeers,
    DialPeer,
    ResetOfflinePeers,
    BanPeer,
    UnbanPeer,
    UnbanAllPeers,
    ListBannedPeers,
    ListConnections,
    ListHeaders,
    CheckDb,
    PeriodStats,
    HeaderStats,
    CalcTiming,
    DiscoverPeer,
    GetBlock,
    SearchUtxo,
    SearchKernel,
    SearchStxo,
    GetMempoolStats,
    GetMempoolState,
    Whoami,
    GetStateInfo,
    Quit,
    Exit,
}

/// This is used to parse commands from the user and execute them
#[derive(Helper, Validator, Highlighter)]
pub struct Parser {
    commands: Vec<String>,
    hinter: HistoryHinter,
    command_handler: Arc<CommandHandler>,
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
    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Parser {
    /// creates a new parser struct
    pub fn new(command_handler: Arc<CommandHandler>) -> Self {
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
    pub fn handle_command(&mut self, command_str: &str, shutdown: &mut Shutdown) {
        if command_str.trim().is_empty() {
            return;
        }

        let del_arg_vec = delimit_command_string(command_str);
        let mut args = command_str.split_whitespace();
        let command = BaseNodeCommand::from_str(args.next().unwrap_or(&"help"));
        if command.is_err() {
            println!("{} is not a valid command, please enter a valid command", command_str);
            println!("Enter help or press tab for available commands");
            return;
        }
        let command = command.unwrap();
        self.process_command(command, args, del_arg_vec, shutdown);
    }

    /// Function to process commands
    fn process_command<'a, I: Iterator<Item = &'a str>>(
        &mut self,
        command: BaseNodeCommand,
        args: I,
        _del_arg_vec: Vec<String>,
        shutdown: &mut Shutdown,
    )
    {
        use BaseNodeCommand::*;
        match command {
            Help => {
                self.print_help(args);
            },
            Status => {
                self.command_handler.status();
            },
            GetStateInfo => {
                self.command_handler.state_info();
            },
            Version => {
                self.command_handler.print_version();
            },
            GetChainMetadata => {
                self.command_handler.get_chain_meta();
            },
            DialPeer => {
                self.process_dial_peer(args);
            },
            DiscoverPeer => {
                self.process_discover_peer(args);
            },
            GetPeer => {
                self.process_get_peer(args);
            },
            ListPeers => {
                self.process_list_peers(args);
            },
            ResetOfflinePeers => {
                self.command_handler.reset_offline_peers();
            },
            CheckDb => {
                self.command_handler.check_db();
            },
            PeriodStats => {
                self.process_period_stats(args);
            },
            HeaderStats => {
                self.process_header_stats(args);
            },
            BanPeer => {
                self.process_ban_peer(args, true);
            },
            UnbanPeer => {
                self.process_ban_peer(args, false);
            },
            UnbanAllPeers => {
                self.command_handler.unban_all_peers();
            },
            ListBannedPeers => {
                self.command_handler.list_banned_peers();
            },
            ListConnections => {
                self.command_handler.list_connections();
            },
            ListHeaders => {
                self.process_list_headers(args);
            },
            CalcTiming => {
                self.process_calc_timing(args);
            },
            GetBlock => {
                self.process_get_block(args);
            },
            SearchUtxo => {
                self.process_search_utxo(args);
            },
            SearchKernel => {
                self.process_search_kernel(args);
            },
            SearchStxo => {
                self.process_search_stxo(args);
            },
            GetMempoolStats => {
                self.command_handler.get_mempool_stats();
            },
            GetMempoolState => {
                self.command_handler.get_mempool_state();
            },
            Whoami => {
                self.command_handler.whoami();
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
    }

    /// Displays the commands or context specific help for a given command
    fn print_help<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let help_for = BaseNodeCommand::from_str(args.next().unwrap_or_default()).unwrap_or(BaseNodeCommand::Help);
        use BaseNodeCommand::*;
        match help_for {
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
            GetChainMetadata => {
                println!("Gets your base node chain meta data");
            },
            DialPeer => {
                println!("Attempt to connect to a known peer");
            },
            DiscoverPeer => {
                println!("Attempt to discover a peer on the Tari network");
            },
            GetPeer => {
                println!("Get all available info about peer");
            },
            ListPeers => {
                println!("Lists the peers that this node knows about");
            },
            ResetOfflinePeers => {
                println!("Clear offline flag from all peers");
            },
            BanPeer => {
                println!("Bans a peer");
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
                println!("Period-stats [start height] [end height]");
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
            CalcTiming => {
                println!("Calculates the time average time taken to mine a given range of blocks.");
            },
            GetBlock => {
                println!("View a block of a height, call this command via:");
                println!("get-block [height of the block] [format]");
                println!(
                    "[height of the block] The height of the block to fetch from the main chain. The genesis block \
                     has height zero."
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
            SearchStxo => {
                println!(
                    "This will search the main chain for the stxo. If the stxo is found, it will print out the block \
                     it was found in."
                );
                println!("search-stxo [hex of commitment of the stxo]");
            },
            GetMempoolStats => {
                println!("Retrieves your mempools stats");
            },
            GetMempoolState => {
                println!("Retrieves your mempools state");
            },
            Whoami => {
                println!(
                    "Display identity information about this node, including: public key, node ID and the public \
                     address"
                );
            },
            Exit | Quit => {
                println!("Exits the base node");
            },
        }
    }

    /// Function to process the get-block command
    fn process_get_block<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        // let command_arg = args.take(4).collect::<Vec<&str>>();
        let height = args.next();
        if height.is_none() {
            self.print_help("get-block".split(' '));
            return;
        }
        let height = match height.unwrap().parse::<u64>().ok() {
            Some(height) => height,
            None => {
                println!("Invalid block height provided. Height must be an integer.");
                self.print_help("get-block".split(' '));
                return;
            },
        };
        let format = match args.next() {
            Some(v) if v.to_ascii_lowercase() == "json" => Format::Json,
            Some(v) if v.to_ascii_lowercase() == "text" => Format::Text,
            None => Format::Text,
            Some(_) => {
                println!("Unrecognized format sspecifier");
                self.print_help("get-block".split(' '));
                return;
            },
        };

        self.command_handler.get_block(height, format)
    }

    /// Function to process the search utxo command
    fn process_search_utxo<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        // let command_arg = args.take(4).collect::<Vec<&str>>();
        let hex = args.next();
        if hex.is_none() {
            self.print_help("search-utxo".split(' '));
            return;
        }
        let commitment = match Commitment::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid commitment provided.");
                self.print_help("search-utxo".split(' '));
                return;
            },
        };
        self.command_handler.search_utxo(commitment)
    }

    /// Function to process the search stxo command
    fn process_search_stxo<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        // let command_arg = args.take(4).collect::<Vec<&str>>();
        let hex = args.next();
        if hex.is_none() {
            self.print_help("search-stxo".split(' '));
            return;
        }
        let commitment = match Commitment::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid commitment provided.");
                self.print_help("search-stxo".split(' '));
                return;
            },
        };

        self.command_handler.search_stxo(commitment)
    }

    /// Function to process the search kernel command
    fn process_search_kernel<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        // let command_arg = args.take(4).collect::<Vec<&str>>();
        let hex = args.next();
        if hex.is_none() {
            self.print_help("search-kernel".split(' '));
            return;
        }
        let public_nonce = match PublicKey::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid public nonce provided.");
                self.print_help("search-kernel".split(' '));
                return;
            },
        };

        let hex = args.next();
        if hex.is_none() {
            self.print_help("search-kernel".split(' '));
            return;
        }
        let signature = match PrivateKey::from_hex(&hex.unwrap().to_string()) {
            Ok(v) => v,
            _ => {
                println!("Invalid signature provided.");
                self.print_help("search-kernel".split(' '));
                return;
            },
        };
        let kernel_sig = Signature::new(public_nonce, signature);

        self.command_handler.search_kernel(kernel_sig)
    }

    /// Function to process the discover-peer command
    fn process_discover_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let dest_pubkey = match args.next().and_then(parse_emoji_id_or_public_key) {
            Some(v) => Box::new(v),
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!("discover-peer [hex public key or emoji id]");
                return;
            },
        };

        self.command_handler.discover_peer(dest_pubkey)
    }

    fn process_get_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let node_id = match args
            .next()
            .map(parse_emoji_id_or_public_key_or_node_id)
            .flatten()
            .map(either_to_node_id)
        {
            Some(n) => n,
            None => {
                println!("Usage: get-peer [NodeId|PublicKey|EmojiId]");
                return;
            },
        };

        self.command_handler.get_peer(node_id)
    }

    /// Function to process the list-peers command
    fn process_list_peers<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let filter = args.next().map(ToOwned::to_owned);

        self.command_handler.list_peers(filter)
    }

    /// Function to process the dial-peer command
    fn process_dial_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let dest_node_id = match args
            .next()
            .and_then(parse_emoji_id_or_public_key_or_node_id)
            .map(either_to_node_id)
        {
            Some(n) => n,
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!("discover-peer [hex public key or emoji id]");
                return;
            },
        };

        self.command_handler.dial_peer(dest_node_id)
    }

    /// Function to process the ban-peer command
    fn process_ban_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I, must_ban: bool) {
        let node_id = match args
            .next()
            .and_then(parse_emoji_id_or_public_key_or_node_id)
            .map(either_to_node_id)
        {
            Some(v) => v,
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!(
                    "ban-peer/unban-peer [hex public key or emoji id] (length of time to ban the peer for in seconds)"
                );
                return;
            },
        };

        let duration = args
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(std::u64::MAX));

        self.command_handler.ban_peer(node_id, duration, must_ban)
    }

    /// Function to process the list-headers command
    fn process_list_headers<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let start = args.next().map(u64::from_str).map(Result::ok).flatten();
        let end = args.next().map(u64::from_str).map(Result::ok).flatten();
        if start.is_none() {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("list-headers [first header height] [last header height]");
            println!("list-headers [amount of headers from chain tip]");
            return;
        }
        let start = start.unwrap();
        self.command_handler.list_headers(start, end)
    }

    /// Function to process the calc-timing command
    fn process_calc_timing<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let start = args.next().map(u64::from_str).map(Result::ok).flatten();
        let end = args.next().map(u64::from_str).map(Result::ok).flatten();
        if start.is_none() {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("calc-timing [first header height] [last header height]");
            println!("calc-timing [number of headers from chain tip]");
            return;
        }
        let start = start.unwrap();
        self.command_handler.calc_timing(start, end)
    }

    fn process_period_stats<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.map(|arg| arg.to_string()).take(3).collect::<Vec<String>>();
        if command_arg.len() != 3 {
            println!("Prints out certain stats to of the block chain, use as follows: ");
            println!(
                "Period-stats [start time in unix timestamp] [end time in unix timestamp] [interval period time in \
                 unix timestamp]"
            );
            return;
        }
        let period_end = match u64::from_str(&command_arg[0]) {
            Ok(v) => v,
            Err(_) => {
                println!("Not a valid number provided");
                return;
            },
        };
        let period_ticker_end = match u64::from_str(&command_arg[1]) {
            Ok(v) => v,
            Err(_) => {
                println!("Not a valid number provided");
                return;
            },
        };
        let period = match u64::from_str(&command_arg[2]) {
            Ok(v) => v,
            Err(_) => {
                println!("Not a valid number provided");
                return;
            },
        };
        self.command_handler.period_stats(period_end, period_ticker_end, period)
    }

    fn process_header_stats<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.map(|arg| arg.to_string()).take(2).collect::<Vec<String>>();
        if command_arg.len() != 2 {
            println!("Prints out certain stats to of the block chain in csv format for easy copy, use as follows: ");
            println!("Period-stats [start height] [end height]");
            return;
        }
        let start_height = match u64::from_str(&command_arg[0]) {
            Ok(v) => v,
            Err(_) => {
                println!("Not a valid number provided");
                return;
            },
        };
        let end_height = match u64::from_str(&command_arg[1]) {
            Ok(v) => v,
            Err(_) => {
                println!("Not a valid number provided");
                return;
            },
        };
        self.command_handler.raw_stats(start_height, end_height)
    }
}
