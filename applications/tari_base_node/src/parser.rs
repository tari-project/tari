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
use chrono::{DateTime, Utc};
use chrono_english::{parse_date_string, Dialect};
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
use tari_comms::types::CommsPublicKey;
use tari_core::{
    tari_utilities::hex::Hex,
    transactions::{
        tari_amount::MicroTari,
        types::{Commitment, PrivateKey, PublicKey, Signature},
    },
};
use tari_shutdown::Shutdown;

/// Enum representing commands used by the basenode
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum BaseNodeCommand {
    Help,
    Version,
    Status,
    GetBalance,
    ListUtxos,
    ListTransactions,
    ListCompletedTransactions,
    CancelTransaction,
    SendTari,
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
    CalcTiming,
    DiscoverPeer,
    GetBlock,
    SearchUtxo,
    SearchKernel,
    SearchStxo,
    GetMempoolStats,
    GetMempoolState,
    Whoami,
    ToggleMining,
    GetMiningState,
    MakeItRain,
    CoinSplit,
    StressTest,
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

const MAKE_IT_RAIN_USAGE: &str = "\nmake-it-rain [Txs/s] [duration (s)] [start amount (uT)] [increment (uT)/Tx] \
                                  [\"start time (UTC)\" / 'now' for immediate start] [public key or emoji id to send \
                                  to] [message]\n";
pub const STRESS_TEST_USAGE: &str = "\nstress-test [command file]\n\nCommand file format:\n  make-it-rain ... (at \
                                     least one required)\n  make-it-rain ... (optional)\n  ...";

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
        del_arg_vec: Vec<String>,
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
            GetBalance => {
                self.command_handler.get_balance();
            },
            ListUtxos => {
                self.command_handler.list_unspent_outputs();
            },
            ListTransactions => {
                self.command_handler.list_transactions();
            },
            ListCompletedTransactions => {
                self.process_list_completed_transactions(args);
            },
            CancelTransaction => {
                self.process_cancel_transaction(args);
            },
            SendTari => {
                self.process_send_tari(args);
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
            ToggleMining => {
                self.command_handler.toggle_mining();
            },
            GetMiningState => {
                self.command_handler.get_mining_state();
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
            MakeItRain => {
                self.process_make_it_rain(del_arg_vec);
            },
            CoinSplit => {
                self.process_coin_split(args);
            },
            StressTest => {
                self.command_handler.stress_test(del_arg_vec);
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
            GetBalance => {
                println!("Gets your balance");
            },
            ListUtxos => {
                println!("List your UTXOs");
            },
            ListTransactions => {
                println!("Print a list of pending inbound and outbound transactions");
            },
            ListCompletedTransactions => {
                println!("Print a list of completed transactions.");
                println!("USAGE: list-completed-transactions [last n] or list-completed-transactions [n] [m]");
            },
            CancelTransaction => {
                println!("Cancel a transaction");
                println!("USAGE: cancel-transaction [transaction ID]");
            },
            SendTari => {
                println!("Sends an amount of Tari to a address call this command via:");
                println!("send-tari [amount of tari to send] [destination public key or emoji id] [optional: msg]");
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
            PeriodStats => {
                println!(
                    "Prints out certain stats to of the block chain in csv format for easy copy, use as follows: "
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
            ToggleMining => {
                println!("Enable or disable the miner on this node, calling this command will toggle the state");
            },
            GetMiningState => println!(
                "Displays the mining state. The hash rate is estimated based on the last measured hash rate and the \
                 number of active mining thread."
            ),
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
            MakeItRain => {
                println!("Sends multiple amounts of Tari to a public wallet address via this command:");
                println!("{}", MAKE_IT_RAIN_USAGE);
            },
            CoinSplit => {
                println!("Constructs a transaction to split a small set of UTXOs into a large set of UTXOs");
            },
            StressTest => {
                println!(
                    "Performs a network stress test by combining coin-split to create test UTXOs and running \
                     make-it-rain afterwards."
                );
                println!("{}", STRESS_TEST_USAGE);
            },
            Exit | Quit => {
                println!("Exits the base node");
            },
        }
    }

    /// Function to process the list utxos command
    fn process_list_completed_transactions<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let n = args.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
        let m = args.next().and_then(|s| s.parse::<usize>().ok());

        self.command_handler.list_completed_transactions(n, m)
    }

    fn process_cancel_transaction<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let tx_id = match args.next().and_then(|s| s.parse::<u64>().ok()) {
            Some(id) => id,
            None => {
                println!("Please enter a valid transaction ID");
                println!("USAGE: cancel-transaction [transaction id]");
                return;
            },
        };

        self.command_handler.cancel_transaction(tx_id)
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

    /// Function to process the coin split command
    fn process_coin_split<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let amount_per_split = args.next().and_then(|v| v.parse::<u64>().ok());
        let split_count = args.next().and_then(|v| v.parse::<usize>().ok());
        if amount_per_split.is_none() | split_count.is_none() {
            println!("Command entered incorrectly, please use the following format: ");
            println!("coin-split [amount of tari to allocated to each UTXO] [number of UTXOs to create]");
            return;
        }
        let amount_per_split: MicroTari = amount_per_split.unwrap().into();
        let split_count = split_count.unwrap();
        self.command_handler.coin_split(amount_per_split, split_count)
    }

    /// Function to process the send transaction command
    fn process_send_tari<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let amount = args.next().and_then(|v| MicroTari::from_str(v).ok());
        if amount.is_none() {
            println!("Please enter a valid amount of tari");
            return;
        }
        let amount: MicroTari = amount.unwrap();

        let key = match args.next() {
            Some(k) => k.to_string(),
            None => {
                println!("Command entered incorrectly, please use the following format: ");
                println!("send_tari [amount of tari to send] [public key or emoji id to send to] [optional message]");
                return;
            },
        };

        let dest_pubkey = match parse_emoji_id_or_public_key(&key) {
            Some(v) => v,
            None => {
                println!("Please enter a valid destination public key or emoji id");
                return;
            },
        };

        // Use the rest of the command line as the message
        let msg = args.collect::<Vec<&str>>().join(" ");

        self.command_handler.send_tari(amount, dest_pubkey, msg)
    }

    /// Function to process the make it rain transaction function
    fn process_make_it_rain(&mut self, command_arg: Vec<String>) {
        // args: [Txs/s] [duration (s)] [start amount (uT)] [increment (uT)/Tx]
        //       [\"start time (UTC)\" / 'now' for immediate start] [public key or emoji id to send to] [message]
        let command_error_msg =
            "Command entered incorrectly, please use the following format:\n".to_owned() + MAKE_IT_RAIN_USAGE;

        // [Txs/s] [duration (s)] [start amount (uT)] [increment (uT)/Tx] [start time (UTC) / 'now'] [public key or
        // emoji id to send to] [message]
        let (tx_per_s, number_of_txs, start_amount, amount_inc, time_utc_start, dest_pubkey, msg) =
            match get_make_it_rain_tx_values(command_arg) {
                Some(v) => {
                    if v.err_msg != "" {
                        println!("\n{}", command_error_msg);
                        println!("\n{}\n", v.err_msg);
                        return;
                    }
                    (
                        v.tx_per_s,
                        v.number_of_txs,
                        v.start_amount,
                        v.amount_inc,
                        v.time_utc_start,
                        v.dest_pubkey,
                        v.msg,
                    )
                },
                None => {
                    println!("Cannot process the 'make-it-rain' command");
                    return;
                },
            };

        self.command_handler.make_it_rain(
            tx_per_s,
            number_of_txs,
            start_amount,
            amount_inc,
            time_utc_start,
            dest_pubkey,
            msg,
        )
    }
}

// Function to get make-it-rain transaction values
pub fn get_make_it_rain_tx_values(command_arg: Vec<String>) -> Option<MakeItRainInputs> {
    if (command_arg.is_empty()) || (command_arg.len() < 6) {
        return Some(MakeItRainInputs {
            err_msg: format!("Expected at least 6 arguments, received {}", command_arg.len()),
            ..Default::default()
        });
    }

    // [number of Txs/s]
    let tx_per_s = command_arg[0].parse::<f64>();
    if tx_per_s.is_err() {
        return Some(MakeItRainInputs {
            err_msg: "Invalid data provided for [number of Txs]".to_string(),
            ..Default::default()
        });
    }
    let tx_per_s = tx_per_s.unwrap();

    // [test duration (s)]
    let duration = command_arg[1].parse::<u32>();
    if duration.is_err() {
        return Some(MakeItRainInputs {
            err_msg: "Invalid data provided for [test duration (s)]".to_string(),
            ..Default::default()
        });
    };
    let duration = duration.unwrap();
    if (tx_per_s * duration as f64) < 1.0 {
        return Some(MakeItRainInputs {
            err_msg: "Invalid data provided for [number of Txs/s] * [test duration (s)], must be >= 1".to_string(),
            ..Default::default()
        });
    }
    let number_of_txs = (tx_per_s * duration as f64) as usize;
    let tx_per_s = tx_per_s.min(25.0); // Maximum rate set to 25/s.

    // [starting amount (uT)]
    let start_amount = command_arg[2].parse::<u64>();
    if start_amount.is_err() {
        return Some(MakeItRainInputs {
            err_msg: "Invalid data provided for [starting amount (uT)]".to_string(),
            ..Default::default()
        });
    }
    let start_amount: MicroTari = start_amount.unwrap().into();

    // [increment (uT)/Tx]
    let amount_inc = command_arg[3].parse::<u64>();
    if amount_inc.is_err() {
        return Some(MakeItRainInputs {
            err_msg: "Invalid data provided for [increment (uT)/Tx]".to_string(),
            ..Default::default()
        });
    }
    let amount_inc: MicroTari = amount_inc.unwrap().into();

    // [start time (UTC) / 'now']
    let time = command_arg[4].to_string();
    let time_utc_ref = Utc::now();
    let mut time_utc_start = Utc::now();
    let datetime = parse_date_string(&time, Utc::now(), Dialect::Uk);
    match datetime {
        Ok(t) => {
            if t > time_utc_ref {
                time_utc_start = t;
            }
        },
        Err(e) => {
            return Some(MakeItRainInputs {
                err_msg: format!("Invalid data provided for [start time (UTC) / 'now']:  {}", e),
                ..Default::default()
            });
        },
    }

    // TODO: Read in recipient address list and custom message from file
    // [public key or emoji id to send to]
    let key = command_arg[5].to_string();
    let dest_pubkey = match parse_emoji_id_or_public_key(&key) {
        Some(v) => v,
        None => {
            return Some(MakeItRainInputs {
                err_msg: "Invalid data provided for [public key or emoji id to send to]".to_string(),
                ..Default::default()
            });
        },
    };

    // [message]
    let mut msg = "".to_string();
    if command_arg.len() > 6 {
        for arg in command_arg.iter().skip(6) {
            msg = msg + arg + " ";
        }
        msg = msg.trim().to_string();
    }

    Some(MakeItRainInputs {
        tx_per_s,
        number_of_txs,
        start_amount,
        amount_inc,
        time_utc_start,
        dest_pubkey,
        msg,
        ..Default::default()
    })
}

#[derive(Clone)]
pub struct MakeItRainInputs {
    pub tx_per_s: f64,
    pub number_of_txs: usize,
    pub start_amount: MicroTari,
    pub amount_inc: MicroTari,
    pub time_utc_start: DateTime<Utc>,
    pub dest_pubkey: CommsPublicKey,
    pub msg: String,
    pub err_msg: String,
}

impl Default for MakeItRainInputs {
    fn default() -> Self {
        Self {
            tx_per_s: f64::default(),
            number_of_txs: usize::default(),
            start_amount: MicroTari::default(),
            amount_inc: MicroTari::default(),
            time_utc_start: Utc::now(),
            dest_pubkey: CommsPublicKey::default(),
            msg: String::default(),
            err_msg: String::default(),
        }
    }
}
