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
use crate::{
    builder::BaseNodeContext,
    table::Table,
    utils,
    utils::{format_duration_basic, format_naive_datetime},
};
use chrono::{DateTime, Utc};
use chrono_english::{parse_date_string, Dialect};
use futures::future::Either;
use itertools::Itertools;
use log::*;
use qrcode::{render::unicode, QrCode};
use regex::Regex;
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    hint::{Hinter, HistoryHinter},
    line_buffer::LineBuffer,
    Context,
};
use rustyline_derive::{Helper, Highlighter, Validator};
use std::{
    fs,
    io::{self, Write},
    ops::Add,
    path::PathBuf,
    str::FromStr,
    string::ToString,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};
use tari_app_utilities::utilities::{parse_emoji_id_or_public_key, parse_emoji_id_or_public_key_or_node_id};
use tari_common::GlobalConfig;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{Peer, PeerFeatures, PeerManager, PeerManagerError, PeerQuery},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::{
    base_node::{
        state_machine_service::states::{PeerMetadata, StatusInfo},
        LocalNodeCommsInterface,
    },
    blocks::BlockHeader,
    mempool::service::LocalMempoolService,
    mining::MinerInstruction,
    tari_utilities::{hex::Hex, message_format::MessageFormat, Hashable},
    transactions::{
        tari_amount::{uT, MicroTari},
        types::{Commitment, PrivateKey, PublicKey, Signature},
    },
};
use tari_crypto::ristretto::pedersen::PedersenCommitmentFactory;
use tari_shutdown::Shutdown;
use tari_wallet::{
    output_manager_service::{error::OutputManagerError, handle::OutputManagerHandle},
    transaction_service::{error::TransactionServiceError, handle::TransactionServiceHandle},
    util::emoji::EmojiId,
};
use tokio::{
    runtime,
    sync::{broadcast::Sender as syncSender, watch},
    time,
};

/// Enum representing commands used by the basenode
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum BaseNodeCommand {
    Help,
    Version,
    GetBalance,
    ListUtxos,
    ListTransactions,
    ListCompletedTransactions,
    CancelTransaction,
    SendTari,
    GetChainMetadata,
    ListPeers,
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
    executor: runtime::Handle,
    discovery_service: DhtDiscoveryRequester,
    base_node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connectivity: ConnectivityRequester,
    commands: Vec<String>,
    hinter: HistoryHinter,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    enable_miner: Arc<AtomicBool>,
    mining_status: Arc<AtomicBool>,
    miner_hashrate: Arc<AtomicU64>,
    miner_instructions: syncSender<MinerInstruction>,
    miner_thread_count: u64,
    state_machine_info: watch::Receiver<StatusInfo>,
    wallet_transaction_service: Option<TransactionServiceHandle>,
    wallet_node_identity: Option<Arc<NodeIdentity>>,
    wallet_peer_manager: Option<Arc<PeerManager>>,
    wallet_connectivity: Option<ConnectivityRequester>,
    wallet_output_service: Option<OutputManagerHandle>,
}

// Import the auto-generated const values from the Manifest and Git
include!(concat!(env!("OUT_DIR"), "/consts.rs"));

const MAKE_IT_RAIN_USAGE: &str = "\nmake-it-rain [Txs/s] [duration (s)] [start amount (uT)] [increment (uT)/Tx] \
                                  [\"start time (UTC)\" / 'now' for immediate start] [public key or emoji id to send \
                                  to] [message]\n";
const STRESS_TEST_USAGE: &str = "\nstress-test [command file]\n\nCommand file format:\n  make-it-rain ... (at least \
                                 one required)\n  make-it-rain ... (optional)\n  ...";

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
    pub fn new(executor: runtime::Handle, ctx: &BaseNodeContext, config: &GlobalConfig) -> Self {
        Parser {
            executor,
            discovery_service: ctx.base_node_dht().discovery_service_requester(),
            base_node_identity: ctx.base_node_identity(),
            peer_manager: ctx.base_node_comms().peer_manager(),
            connectivity: ctx.base_node_comms().connectivity(),
            commands: BaseNodeCommand::iter().map(|x| x.to_string()).collect(),
            hinter: HistoryHinter {},
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            enable_miner: ctx.miner_enabled(),
            mining_status: ctx.mining_status(),
            miner_hashrate: ctx.miner_hashrate(),
            miner_instructions: ctx.miner_instruction_events(),
            miner_thread_count: config.num_mining_threads as u64,
            state_machine_info: ctx.get_state_machine_info_channel(),
            wallet_node_identity: ctx.wallet_node_identity(),
            wallet_peer_manager: ctx.wallet_comms().map(|wc| wc.peer_manager()),
            wallet_connectivity: ctx.wallet_comms().map(|wc| wc.connectivity()),
            wallet_output_service: ctx.output_manager(),
            wallet_transaction_service: ctx.wallet_transaction_service(),
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
            GetStateInfo => {
                self.process_state_info();
            },
            Version => {
                self.print_version();
            },
            GetBalance => {
                self.process_get_balance();
            },
            ListUtxos => {
                self.process_list_unspent_outputs();
            },
            ListTransactions => {
                self.process_list_transactions();
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
                self.process_get_chain_meta();
            },
            DiscoverPeer => {
                self.process_discover_peer(args);
            },
            ListPeers => {
                self.process_list_peers(args);
            },
            ResetOfflinePeers => {
                self.process_reset_offline_peers();
            },
            CheckDb => {
                self.process_check_db();
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
                self.process_unban_all_peers();
            },
            ListBannedPeers => {
                self.process_list_banned_peers();
            },
            ListConnections => {
                self.process_list_connections();
            },
            ListHeaders => {
                self.process_list_headers(args);
            },
            CalcTiming => {
                self.process_calc_timing(args);
            },
            ToggleMining => {
                self.process_toggle_mining();
            },
            GetMiningState => {
                self.process_get_mining_state();
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
                self.process_get_mempool_stats();
            },
            GetMempoolState => {
                self.process_get_mempool_state();
            },
            Whoami => {
                self.process_whoami();
            },
            MakeItRain => {
                self.process_make_it_rain(del_arg_vec);
            },
            CoinSplit => {
                self.process_coin_split(args);
            },
            StressTest => {
                self.process_stress_test(del_arg_vec);
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
            DiscoverPeer => {
                println!("Attempt to discover a peer on the Tari network");
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

    /// Function to process the get-balance command
    fn process_get_balance(&mut self) {
        if let Some(mut handler) = self.wallet_output_service.clone() {
            self.executor.spawn(async move {
                match handler.get_balance().await {
                    Err(e) => {
                        println!("Something went wrong");
                        warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
                        return;
                    },
                    Ok(data) => println!("Balances:\n{}", data),
                };
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    /// Function process the version command
    fn print_version(&mut self) {
        println!("Version: {}", VERSION);
        println!("Author: {}", AUTHOR);
    }

    /// Function to process the get-state-info command
    fn process_state_info(&mut self) {
        let mut channel = self.state_machine_info.clone();
        self.executor.spawn(async move {
            match channel.recv().await {
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Error communicating with state machine, channel could have been closed"
                    );
                    return;
                },
                Some(data) => println!("Current state machine state:\n{}", data),
            };
        });
    }

    /// Function to process the list utxos command
    fn process_list_unspent_outputs(&mut self) {
        if let Some(mut handler2) = self.wallet_output_service.clone() {
            let mut handler1 = self.node_service.clone();

            self.executor.spawn(async move {
                let current_height = match handler1.get_metadata().await {
                    Err(err) => {
                        println!("Failed to retrieve chain metadata: {:?}", err);
                        warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err);
                        return;
                    },
                    Ok(data) => data.height_of_longest_chain.unwrap() as i64,
                };
                match handler2.get_unspent_outputs().await {
                    Err(e) => {
                        println!("Something went wrong");
                        warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
                        return;
                    },
                    Ok(unspent_outputs) => {
                        if !unspent_outputs.is_empty() {
                            println!(
                                "\nYou have {} UTXOs: (value, commitment, mature in ? blocks, flags)",
                                unspent_outputs.len()
                                uo.commitment().to_hex(),
                            );
                            let factory = PedersenCommitmentFactory::default();
                            for uo in unspent_outputs.iter() {
                                let mature_in = std::cmp::max(uo.features.maturity as i64 - current_height, 0);
                                println!(
                                    "   {}, {}, {:>3}, {:?}",
                                    uo.value,
                                    uo.as_transaction_input(&factory, OutputFeatures::default())
                                        .commitment
                                        .to_hex(),
                                    mature_in,
                                    uo.features.flags
                                );
                            }
                            println!();
                        } else {
                            println!("\nNo valid UTXOs found at this time\n");
                        }
                    },
                };
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    fn process_list_transactions(&mut self) {
        if let Some(mut transactions) = self.wallet_transaction_service.clone() {
            self.executor.spawn(async move {
                println!("Inbound Transactions");
                match transactions.get_pending_inbound_transactions().await {
                    Ok(transactions) => {
                        if transactions.is_empty() {
                            println!("No pending inbound transactions found.");
                        } else {
                            let mut table = Table::new();
                            table.set_titles(vec![
                                "Transaction ID",
                                "Source Public Key",
                                "Amount",
                                "Status",
                                "Receiver State",
                                "Timestamp",
                                "Message",
                            ]);
                            for (tx_id, txn) in transactions {
                                table.add_row(row![
                                    tx_id,
                                    txn.source_public_key,
                                    txn.amount,
                                    txn.status,
                                    txn.receiver_protocol.state,
                                    format_naive_datetime(&txn.timestamp),
                                    txn.message
                                ]);
                            }

                            table.print_std();
                        }
                    },
                    Err(err) => {
                        println!("Failed to retrieve inbound transactions: {:?}", err);
                        return;
                    },
                }

                println!();
                println!("Outbound Transactions");
                match transactions.get_pending_outbound_transactions().await {
                    Ok(transactions) => {
                        if transactions.is_empty() {
                            println!("No pending outbound transactions found.");
                            return;
                        }

                        let mut table = Table::new();
                        table.set_titles(vec![
                            "Transaction ID",
                            "Dest Public Key",
                            "Amount",
                            "Fee",
                            "Status",
                            "Sender State",
                            "Timestamp",
                            "Message",
                        ]);
                        for (tx_id, txn) in transactions {
                            table.add_row(row![
                                tx_id,
                                txn.destination_public_key,
                                txn.amount,
                                txn.fee,
                                txn.status,
                                txn.sender_protocol,
                                format_naive_datetime(&txn.timestamp),
                                txn.message
                            ]);
                        }

                        table.print_std();
                    },
                    Err(err) => {
                        println!("Failed to retrieve inbound transactions: {:?}", err);
                        return;
                    },
                }
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    fn process_list_completed_transactions<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        if let Some(mut transactions) = self.wallet_transaction_service.clone() {
            let n = args.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
            let m = args.next().and_then(|s| s.parse::<usize>().ok());

            self.executor.spawn(async move {
                match transactions.get_completed_transactions().await {
                    Ok(transactions) => {
                        if transactions.is_empty() {
                            println!("No completed transactions found.");
                            return;
                        }
                        // TODO: This doesn't scale well because hashmap has a random ordering. Support for this query
                        //       should be added at the database level
                        let mut transactions = transactions.into_iter().map(|(_, txn)| txn).collect::<Vec<_>>();
                        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                        let transactions = match m {
                            Some(m) => transactions.into_iter().skip(n).take(m).collect::<Vec<_>>(),
                            None => transactions.into_iter().take(n).collect::<Vec<_>>(),
                        };

                        let mut table = Table::new();
                        table.set_titles(vec![
                            "Transaction ID",
                            "Sender",
                            "Receiver",
                            "Amount",
                            "Fee",
                            "Status",
                            "Timestamp",
                            "Message",
                        ]);
                        for txn in transactions {
                            table.add_row(row![
                                txn.tx_id,
                                txn.source_public_key,
                                txn.destination_public_key,
                                txn.amount,
                                txn.fee,
                                txn.status,
                                format_naive_datetime(&txn.timestamp),
                                txn.message
                            ]);
                        }

                        table.print_std();
                    },
                    Err(err) => {
                        println!("Failed to retrieve inbound transactions: {:?}", err);
                        return;
                    },
                }
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    fn process_cancel_transaction<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        if let Some(mut transactions) = self.wallet_transaction_service.clone() {
            let tx_id = match args.next().and_then(|s| s.parse::<u64>().ok()) {
                Some(id) => id,
                None => {
                    println!("Please enter a valid transaction ID");
                    println!("USAGE: cancel-transaction [transaction id]");
                    return;
                },
            };

            self.executor.spawn(async move {
                match transactions.cancel_transaction(tx_id).await {
                    Ok(_) => {
                        println!("Transaction {} successfully cancelled", tx_id);
                    },
                    Err(err) => {
                        println!("Failed to cancel transaction: {:?}", err);
                    },
                }
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    /// Function to process the get-chain-metadata command
    fn process_get_chain_meta(&mut self) {
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_metadata().await {
                Err(err) => {
                    println!("Failed to retrieve chain metadata: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err);
                    return;
                },
                Ok(data) => println!("{}", data),
            };
        });
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
        enum Format {
            Json,
            Text,
        }
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
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks(vec![height]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match (data.pop(), format) {
                    (Some(historical_block), Format::Text) => println!("{}", historical_block.block),
                    (Some(historical_block), Format::Json) => println!(
                        "{}",
                        historical_block
                            .block
                            .to_json()
                            .unwrap_or_else(|_| "Error deserializing block".into())
                    ),
                    (None, _) => println!("Block not found at height {}", height),
                },
            };
        });
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
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks_with_utxos(vec![commitment.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block),
                    _ => println!(
                        "Pruned node: utxo found, but lock not found for utxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
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
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks_with_stxos(vec![commitment.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block),
                    _ => println!(
                        "Pruned node: stxo found, but block not found for stxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
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
        let kernel = Signature::new(public_nonce, signature);
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks_with_kernels(vec![kernel.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block),
                    _ => println!(
                        "Pruned node: kernel found, but block not found for kernel signature {}",
                        kernel.get_signature().to_hex()
                    ),
                },
            };
        });
    }

    /// Function to process the get-mempool-stats command
    fn process_get_mempool_stats(&mut self) {
        let mut handler = self.mempool_service.clone();
        self.executor.spawn(async move {
            match handler.get_mempool_stats().await {
                Ok(stats) => println!("{}", stats),
                Err(err) => {
                    println!("Failed to retrieve mempool stats: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with local mempool: {:?}", err,);
                    return;
                },
            };
        });
    }

    /// Function to process the get-mempool-state command
    fn process_get_mempool_state(&mut self) {
        let mut handler = self.mempool_service.clone();
        self.executor.spawn(async move {
            match handler.get_mempool_state().await {
                Ok(state) => println!("{}", state),
                Err(err) => {
                    println!("Failed to retrieve mempool state: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with local mempool: {:?}", err,);
                    return;
                },
            };
        });
    }

    /// Function to process the discover-peer command
    fn process_discover_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let mut dht = self.discovery_service.clone();

        let dest_pubkey = match args.next().and_then(parse_emoji_id_or_public_key) {
            Some(v) => Box::new(v),
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!("discover-peer [hex public key or emoji id]");
                return;
            },
        };

        self.executor.spawn(async move {
            let start = Instant::now();
            println!("🌎 Peer discovery started.");

            match dht
                .discover_peer(dest_pubkey.clone(), NodeDestination::PublicKey(dest_pubkey))
                .await
            {
                Ok(p) => {
                    println!("⚡️ Discovery succeeded in {}ms!", start.elapsed().as_millis());
                    println!("This peer was found:");
                    println!("{}", p);
                },
                Err(err) => {
                    println!("💀 Discovery failed: '{:?}'", err);
                },
            }
        });
    }

    /// Function to process the list-peers command
    fn process_list_peers<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let peer_manager = self.peer_manager.clone();
        let filter = args.next().map(ToOwned::to_owned);

        self.executor.spawn(async move {
            let mut query = PeerQuery::new();
            if let Some(f) = filter {
                let filter = f.to_lowercase();
                query = query.select_where(move |p| match filter.as_str() {
                    "basenode" | "basenodes" | "base_node" | "base-node" | "bn" => {
                        p.features == PeerFeatures::COMMUNICATION_NODE
                    },
                    "wallet" | "wallets" | "w" => p.features == PeerFeatures::COMMUNICATION_CLIENT,
                    _ => false,
                })
            }
            match peer_manager.perform_query(query).await {
                Ok(peers) => {
                    let num_peers = peers.len();
                    println!();
                    let mut table = Table::new();
                    table.set_titles(vec!["NodeId", "Public Key", "Flags", "Role", "User Agent", "Info"]);

                    for peer in peers {
                        let info_str = {
                            let mut s = vec![];

                            if peer.is_offline() {
                                if !peer.is_banned() {
                                    s.push("OFFLINE".to_string());
                                }
                            } else if let Some(dt) = peer.last_seen() {
                                s.push(format!(
                                    "LAST_SEEN = {}",
                                    Utc::now()
                                        .signed_duration_since(dt)
                                        .to_std()
                                        .map(format_duration_basic)
                                        .unwrap_or_else(|_| "?".into())
                                ));
                            }

                            if let Some(dt) = peer.banned_until() {
                                s.push(format!(
                                    "BANNED({}, {})",
                                    dt.signed_duration_since(Utc::now().naive_utc())
                                        .to_std()
                                        .map(format_duration_basic)
                                        .unwrap_or_else(|_| "∞".to_string()),
                                    peer.banned_reason
                                ));
                            }

                            if let Some(metadata) = peer
                                .get_metadata(1)
                                .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                            {
                                s.push(format!(
                                    "chain height = {}",
                                    metadata.metadata.height_of_longest_chain()
                                ));
                            }

                            if s.is_empty() {
                                "--".to_string()
                            } else {
                                s.join(", ")
                            }
                        };
                        table.add_row(row![
                            peer.node_id,
                            peer.public_key,
                            format!("{:?}", peer.flags),
                            {
                                if peer.features == PeerFeatures::COMMUNICATION_CLIENT {
                                    "Wallet"
                                } else {
                                    "Base node"
                                }
                            },
                            Some(peer.user_agent)
                                .map(|ua| if ua.is_empty() { "<unknown>".to_string() } else { ua })
                                .unwrap(),
                            info_str,
                        ]);
                    }
                    table.print_std();

                    println!("{} peer(s) known by this node", num_peers);
                },
                Err(err) => {
                    println!("Failed to list peers: {:?}", err);
                    error!(target: LOG_TARGET, "Could not list peers: {:?}", err);
                    return;
                },
            }
        });
    }

    /// Function to process the ban-peer command
    fn process_ban_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I, must_ban: bool) {
        let node_key = match args.next().and_then(parse_emoji_id_or_public_key_or_node_id) {
            Some(v) => v,
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!(
                    "ban-peer/unban-peer [hex public key or emoji id] (length of time to ban the peer for in seconds)"
                );
                return;
            },
        };

        match &node_key {
            Either::Left(public_key) => {
                if let Some(wni) = self.wallet_node_identity.clone() {
                    if wni.public_key() == public_key {
                        println!("Cannot ban our own wallet");
                        return;
                    }
                }

                if self.base_node_identity.public_key() == public_key {
                    println!("Cannot ban our own node");
                    return;
                }
            },
            Either::Right(node_id) => {
                if let Some(wni) = self.wallet_node_identity.clone() {
                    if wni.node_id() == node_id {
                        println!("Cannot ban our own wallet");
                        return;
                    }
                }
                if self.base_node_identity.node_id() == node_id {
                    println!("Cannot ban our own node");
                    return;
                }
            },
        }

        let mut connectivity = self.connectivity.clone();
        let wallet_connectivity = self.wallet_connectivity.clone();
        let peer_manager = self.peer_manager.clone();
        let wallet_peer_manager = self.wallet_peer_manager.clone();

        let duration = args
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(std::u64::MAX));

        self.executor.spawn(async move {
            let node_id = match node_key {
                Either::Left(public_key) => match peer_manager.find_by_public_key(&public_key).await {
                    Ok(peer) => peer.node_id,
                    Err(err) if err.is_peer_not_found() => {
                        println!("Peer not found in base node");
                        return;
                    },
                    Err(err) => {
                        println!("Failed to ban peer: {:?}", err);
                        error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                        return;
                    },
                },
                Either::Right(node_id) => node_id,
            };

            if must_ban {
                match connectivity
                    .ban_peer(node_id.clone(), duration, "UI manual ban".to_string())
                    .await
                {
                    Ok(_) => println!("Peer was banned in base node."),
                    Err(err) => {
                        println!("Failed to ban peer: {:?}", err);
                        error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                    },
                }

                if let Some(mut wallet_connectivity) = wallet_connectivity {
                    match wallet_connectivity
                        .ban_peer(node_id, duration, "UI manual ban".to_string())
                        .await
                    {
                        Ok(_) => println!("Peer was banned in wallet."),
                        Err(err) => {
                            println!("Failed to ban peer: {:?}", err);
                            error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                        },
                    }
                }
            } else {
                match peer_manager.unban_peer(&node_id).await {
                    Ok(_) => {
                        println!("Peer ban was removed from base node.");
                    },
                    Err(err) if err.is_peer_not_found() => {
                        println!("Peer not found in base node");
                    },
                    Err(err) => {
                        println!("Failed to ban peer: {:?}", err);
                        error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                    },
                }

                if let Some(wallet_peer_manager) = wallet_peer_manager {
                    match wallet_peer_manager.unban_peer(&node_id).await {
                        Ok(_) => {
                            println!("Peer ban was removed from wallet.");
                        },
                        Err(err) if err.is_peer_not_found() => {
                            println!("Peer not found in wallet");
                        },
                        Err(err) => {
                            println!("Failed to ban peer: {:?}", err);
                            error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                        },
                    }
                }
            }
        });
    }

    fn process_unban_all_peers(&mut self) {
        let peer_manager = self.peer_manager.clone();
        let wallet_peer_manager = self.wallet_peer_manager.clone();
        self.executor.spawn(async move {
            async fn unban_all(pm: &PeerManager) -> usize {
                let query = PeerQuery::new().select_where(|p| p.is_banned());
                match pm.perform_query(query).await {
                    Ok(peers) => {
                        let num_peers = peers.len();
                        for peer in peers {
                            if let Err(err) = pm.unban_peer(&peer.node_id).await {
                                println!("Failed to unban peer: {}", err);
                            }
                        }
                        num_peers
                    },
                    Err(err) => {
                        println!("Failed to unban peers: {}", err);
                        0
                    },
                }
            }

            let n = unban_all(&peer_manager).await;
            println!("Unbanned {} peer(s) from node", n);
            if let Some(wallet_peer_manager) = wallet_peer_manager {
                let n = unban_all(&wallet_peer_manager).await;
                println!("Unbanned {} peer(s) from wallet", n);
            }
        });
    }

    fn process_list_banned_peers(&mut self) {
        let peer_manager = self.peer_manager.clone();
        let wallet_peer_manager = self.wallet_peer_manager.clone();
        self.executor.spawn(async move {
            async fn banned_peers(pm: &PeerManager) -> Result<Vec<Peer>, PeerManagerError> {
                let query = PeerQuery::new().select_where(|p| p.is_banned());
                pm.perform_query(query).await
            }

            match banned_peers(&peer_manager).await {
                Ok(banned) => {
                    if banned.is_empty() {
                        println!("No peers banned from node.")
                    } else {
                        println!("Peers banned from node ({}):", banned.len());
                        for peer in banned {
                            println!("{}", peer);
                        }
                    }
                },
                Err(e) => println!("Error listing peers: {}", e),
            }

            if let Some(wallet_peer_manager) = wallet_peer_manager {
                match banned_peers(&wallet_peer_manager).await {
                    Ok(banned) => {
                        if banned.is_empty() {
                            println!("No peers banned from wallet.")
                        } else {
                            println!("Peers banned from wallet ({}):", banned.len());
                            for peer in banned {
                                println!("{}", peer);
                            }
                        }
                    },
                    Err(e) => println!("Error listing peers: {}", e),
                }
            }
        });
    }

    /// Function to process the list-connections command
    fn process_list_connections(&self) {
        let mut connectivity = self.connectivity.clone();
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            match connectivity.get_active_connections().await {
                Ok(conns) if conns.is_empty() => {
                    println!("No active peer connections.");
                },
                Ok(conns) => {
                    println!();
                    let num_connections = conns.len();
                    let mut table = Table::new();
                    table.set_titles(vec![
                        "NodeId",
                        "Public Key",
                        "Address",
                        "Direction",
                        "Age",
                        "Role",
                        "User Agent",
                    ]);
                    for conn in conns {
                        let peer = peer_manager
                            .find_by_node_id(conn.peer_node_id())
                            .await
                            .expect("Unexpected peer database error or peer not found");

                        table.add_row(row![
                            peer.node_id,
                            peer.public_key,
                            conn.address(),
                            conn.direction(),
                            format_duration_basic(conn.age()),
                            {
                                if peer.features == PeerFeatures::COMMUNICATION_CLIENT {
                                    "Wallet"
                                } else {
                                    "Base node"
                                }
                            },
                            Some(peer.user_agent)
                                .map(|ua| if ua.is_empty() { "<unknown>".to_string() } else { ua })
                                .unwrap()
                        ]);
                    }

                    table.print_std();

                    println!("{} active connection(s)", num_connections);
                },
                Err(err) => {
                    println!("Failed to list connections: {:?}", err);
                    error!(target: LOG_TARGET, "Could not list connections: {:?}", err);
                    return;
                },
            }
        });
    }

    fn process_reset_offline_peers(&self) {
        let peer_manager = self.peer_manager.clone();
        self.executor.spawn(async move {
            let result = peer_manager
                .update_each(|mut peer| {
                    if peer.is_offline() {
                        peer.set_offline(false);
                        Some(peer)
                    } else {
                        None
                    }
                })
                .await;

            match result {
                Ok(num_updated) => {
                    println!("{} peer(s) were unmarked as offline.", num_updated);
                },
                Err(err) => {
                    println!("Failed to clear offline peer states: {:?}", err);
                    error!(target: LOG_TARGET, "{:?}", err);
                    return;
                },
            }
        });
    }

    /// Function to process the toggle-mining command
    fn process_toggle_mining(&mut self) {
        // 'enable_miner' should not be changed directly; this is done indirectly via miner instructions,
        // while 'mining_status' will reflect if mining is happening or not
        if self.wallet_output_service.is_some() {
            let enable_miner = self.enable_miner.clone();
            let mining_status = self.mining_status.clone();
            let miner_instructions = self.miner_instructions.clone();
            self.executor.spawn(async move {
                let new_state = !enable_miner.load(Ordering::SeqCst);
                // The event channel can interrupt the mining thread timeously to stop or start mining
                let _ = match new_state {
                    true => {
                        println!("Mining requested to be turned ON");
                        miner_instructions.send(MinerInstruction::StartMining).map_err(|e| {
                            error!(
                                target: LOG_TARGET,
                                "Could not send 'StartMining' instruction to miner. {:?}.", e
                            );
                            e
                        })
                    },
                    false => {
                        println!("Mining requested to be turned OFF");
                        miner_instructions.send(MinerInstruction::PauseMining).map_err(|e| {
                            error!(
                                target: LOG_TARGET,
                                "Could not send 'PauseMining' instruction to miner. {:?}.", e
                            );
                            e
                        })
                    },
                };
                debug!(
                    target: LOG_TARGET,
                    "Mining state requested to be switched to {}", new_state
                );

                // Verify the mining status
                let mut attempts = 0;
                const DELAY: u64 = 2500;
                const WAIT_CYCLES: usize = 50;
                loop {
                    tokio::time::delay_for(Duration::from_millis(DELAY)).await;
                    if new_state == mining_status.load(Ordering::SeqCst) {
                        match new_state {
                            true => println!("Mining is ON"),
                            false => println!("Mining is OFF"),
                        }
                        break;
                    }
                    attempts += 1;
                    if attempts > WAIT_CYCLES {
                        match new_state {
                            true => println!(
                                "Mining could not be turned ON in {:.1} s (mining enabled is set to {})",
                                DELAY as f32 * attempts as f32 / 1000.0,
                                enable_miner.load(Ordering::SeqCst)
                            ),
                            false => println!(
                                "Mining could not to be turned OFF in {:.1} s (mining enabled is set to {})",
                                DELAY as f32 * attempts as f32 / 1000.0,
                                enable_miner.load(Ordering::SeqCst)
                            ),
                        }
                        break;
                    }
                }
            });
        } else {
            println!("Cannot complete command, Wallet is disabled so Mining is also disabled");
        }
    }

    /// Function to process the get_mining_state command
    fn process_get_mining_state(&mut self) {
        if self.wallet_output_service.is_some() {
            let cur_state = self.enable_miner.load(Ordering::SeqCst);
            let mining_status = self.mining_status.load(Ordering::SeqCst);
            match cur_state {
                true => println!("Mining is ENABLED by the user"),
                false => println!("Mining is DISABLED by the user"),
            }
            match mining_status {
                true => println!("Mining state is currently ON"),
                false => println!("Mining state is currently OFF"),
            }
            let hashrate = self.miner_hashrate.load(Ordering::SeqCst);
            let total_hashrate = (self.miner_thread_count * hashrate) as f64 / 1_000_000.0;
            println!("Mining hash rate is: {:.6} MH/s", total_hashrate);
        } else {
            println!("Cannot complete command, Wallet is disabled so Mining is also disabled");
        }
    }

    /// Function to process the list-headers command
    fn process_list_headers<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.map(|arg| arg.to_string()).take(4).collect::<Vec<String>>();
        if (command_arg.is_empty()) || (command_arg.len() > 2) {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("list-headers [first header height] [last header height]");
            println!("list-headers [amount of headers from chain tip]");
            return;
        }
        let handler = self.node_service.clone();
        self.executor.spawn(async move {
            let headers = Parser::get_headers(handler, command_arg).await;
            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
        });
    }

    /// Helper function to convert an array from command_arg to a Vec<u64> of header heights
    async fn cmd_arg_to_header_heights(handler: LocalNodeCommsInterface, command_arg: Vec<String>) -> Vec<u64> {
        let height_ranges: Result<Vec<u64>, _> = command_arg.iter().map(|v| u64::from_str(v)).collect();
        match height_ranges {
            Ok(height_ranges) => {
                if height_ranges.len() == 2 {
                    let start = height_ranges[0];
                    let end = height_ranges[1];
                    BlockHeader::get_height_range(start, end)
                } else {
                    match BlockHeader::get_heights_from_tip(handler, height_ranges[0]).await {
                        Ok(heights) => heights,
                        Err(_) => {
                            println!("Error communicating with comm interface");
                            Vec::new()
                        },
                    }
                }
            },
            Err(_e) => {
                println!("Invalid number provided");
                Vec::new()
            },
        }
    }

    /// Function to process the get-headers command
    async fn get_headers(mut handler: LocalNodeCommsInterface, command_arg: Vec<String>) -> Vec<BlockHeader> {
        let heights = Self::cmd_arg_to_header_heights(handler.clone(), command_arg).await;
        match handler.get_headers(heights).await {
            Err(err) => {
                println!("Failed to retrieve headers: {:?}", err);
                warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                Vec::new()
            },
            Ok(data) => data,
        }
    }

    /// Function to process the calc-timing command
    fn process_calc_timing<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.map(|arg| arg.to_string()).take(4).collect::<Vec<String>>();
        if (command_arg.is_empty()) || (command_arg.len() > 2) {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("calc-timing [first header height] [last header height]");
            println!("calc-timing [number of headers from chain tip]");
            return;
        }
        let handler = self.node_service.clone();

        self.executor.spawn(async move {
            let headers = Parser::get_headers(handler, command_arg).await;
            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        });
    }

    /// Function to process the check-db command
    fn process_check_db(&mut self) {
        // Todo, add calls to ask peers for missing data
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain.expect("Could not retrieve chain height");
            let mut missing_blocks = Vec::new();
            let mut missing_headers = Vec::new();
            print!("Searching for height: ");
            // We need to check every header, but not every block.
            let horizon_height = meta.horizon_block(height);
            while height > 0 {
                print!("{}", height);
                io::stdout().flush().unwrap();
                // we can only check till the pruning horizon, 0 is archive node so it needs to check every block.
                if height > horizon_height {
                    match node.get_blocks(vec![height]).await {
                        Err(_err) => {
                            missing_blocks.push(height);
                        },
                        Ok(mut data) => match data.pop() {
                            // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                            // logging it.
                            Some(_historical_block) => {},
                            None => missing_blocks.push(height),
                        },
                    };
                }
                height -= 1;
                let next_header = node.get_headers(vec![height]).await;
                if next_header.is_err() {
                    // this header is missing, so we stop here and need to ask for this header
                    missing_headers.push(height);
                };
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            }
            println!("Complete");
            for missing_block in missing_blocks {
                println!("Missing block at height: {}", missing_block);
            }
            for missing_header_height in missing_headers {
                println!("Missing header at height: {}", missing_header_height)
            }
        });
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
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain.expect("Could not retrieve chain height");
            // Currently gets the stats for: tx count, hash rate estimation, target difficulty, solvetime.
            let mut results: Vec<(usize, f64, u64, u64, usize)> = Vec::new();
            let period_end = match u64::from_str(&command_arg[0]) {
                Ok(v) => v,
                Err(_) => {
                    println!("Not a valid number provided");
                    return;
                },
            };
            let mut period_ticker_end = match u64::from_str(&command_arg[1]) {
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
            let mut period_ticker_start = period_ticker_end - period;
            let mut period_tx_count = 0;
            let mut period_block_count = 0;
            let mut period_hash = 0.0;
            let mut period_difficulty = 0;
            let mut period_solvetime = 0;
            print!("Searching for height: ");
            while height > 0 {
                print!("{}", height);
                io::stdout().flush().unwrap();

                let block = match node.get_blocks(vec![height]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block.block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                let prev_block = match node.get_blocks(vec![height - 1]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block.block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                height -= 1;
                if block.header.timestamp.as_u64() > period_ticker_end {
                    print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
                    continue;
                };
                while block.header.timestamp.as_u64() < period_ticker_start {
                    results.push((
                        period_tx_count,
                        period_hash,
                        period_difficulty,
                        period_solvetime,
                        period_block_count,
                    ));
                    period_tx_count = 0;
                    period_block_count = 0;
                    period_hash = 0.0;
                    period_difficulty = 0;
                    period_solvetime = 0;
                    period_ticker_end -= period;
                    period_ticker_start -= period;
                }
                period_tx_count += block.body.kernels().len() - 1;
                period_block_count += 1;
                let st = if prev_block.header.timestamp.as_u64() >= block.header.timestamp.as_u64() {
                    1.0
                } else {
                    (block.header.timestamp.as_u64() - prev_block.header.timestamp.as_u64()) as f64
                };
                let diff = block.header.pow.target_difficulty.as_u64();
                period_difficulty += diff;
                period_solvetime += st as u64;
                period_hash += diff as f64 / st / 1_000_000.0;
                if period_ticker_end <= period_end {
                    break;
                }
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            }
            println!("Complete");
            println!("Results of tx count, hash rate estimation, target difficulty, solvetime, block count");
            for data in results {
                println!("{},{},{},{},{}", data.0, data.1, data.2, data.3, data.4);
            }
        });
    }

    /// Function to process the whoami command
    fn process_whoami(&self) {
        if let Some(wallet_node_identity) = self.wallet_node_identity.clone() {
            println!("======== Wallet ==========");
            println!("{}", wallet_node_identity);
            let emoji_id = EmojiId::from_pubkey(&wallet_node_identity.public_key());
            println!("Emoji ID: {}", emoji_id);
            println!();
            // TODO: Pass the network in as a var
            let qr_link = format!("tari://ridcully/pubkey/{}", &wallet_node_identity.public_key().to_hex());
            let code = QrCode::new(qr_link).unwrap();
            let image = code
                .render::<unicode::Dense1x2>()
                .dark_color(unicode::Dense1x2::Dark)
                .light_color(unicode::Dense1x2::Light)
                .build();
            println!("{}", image);
            println!();
        }
        println!("======== Base Node ==========");
        println!("{}", self.base_node_identity);
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

        // Use output manager service to get utxo and create the coin split transaction
        let mut output_manager = match self.wallet_output_service.clone() {
            Some(v) => v,
            _ => {
                println!("Error: Problem with OutputManagerHandle");
                return;
            },
        };
        let mut txn_service = match self.wallet_transaction_service.clone() {
            Some(v) => v,
            _ => {
                println!("Error: Problem with TransactionServiceHandle");
                return;
            },
        };
        self.executor.spawn(async move {
            coin_split(&mut output_manager, &mut txn_service, amount_per_split, split_count).await;
        });
    }

    /// Function to process the send transaction command
    fn process_send_tari<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        if let Some(wallet_transaction_service) = self.wallet_transaction_service.clone() {
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
                    println!(
                        "send_tari [amount of tari to send] [public key or emoji id to send to] [optional message]"
                    );
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

            self.executor.spawn(async move {
                send_tari(amount, dest_pubkey.clone(), msg.clone(), wallet_transaction_service).await;
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    /// Function to process the stress test transaction function
    fn process_stress_test(&mut self, command_arg: Vec<String>) {
        // args: [command file]
        let command_error_msg =
            "Command entered incorrectly, please use the following format:\n".to_owned() + STRESS_TEST_USAGE;

        if command_arg.is_empty() {
            println!("{}\n", command_error_msg);
            println!("Expected at least 1 argument\n");
            return;
        }

        // Read [command file]
        let command_file = PathBuf::from(command_arg[0].as_str());
        if !command_file.is_file() {
            println!("{}\n", command_error_msg);
            println!(
                "Invalid data provided for [command file], '{}' does not exist\n",
                command_file.as_path().display()
            );
            return;
        }
        let script = match fs::read_to_string(command_file.clone()) {
            Ok(f) => f,
            _ => {
                println!("{}\n", command_error_msg);
                println!(
                    "Invalid data provided for [command file], '{}' could not be read!\n",
                    command_file.as_path().display()
                );
                return;
            },
        };
        if script.is_empty() {
            println!("{}\n", command_error_msg);
            println!(
                "Invalid data provided for [command file], '{}' is empty!\n",
                command_file.as_path().display()
            );
            return;
        };
        let mut make_it_rain_commands = Vec::new();
        for command in script.lines() {
            if command.starts_with("make-it-rain ") {
                make_it_rain_commands.push(delimit_command_string(command));
                if (make_it_rain_commands[make_it_rain_commands.len() - 1].is_empty()) ||
                    (make_it_rain_commands[make_it_rain_commands.len() - 1].len() < 6)
                {
                    println!("{}", command_error_msg);
                    println!(
                        "'make-it-rain' command expected at least 6 arguments, received {}\n  '{}'\n",
                        command_arg.len(),
                        command
                    );
                    return;
                }
            }
        }
        let command_error_msg = "Invalid data provided in '".to_owned() +
            command_file.as_path().to_str().unwrap() +
            "':\n" +
            STRESS_TEST_USAGE;
        if make_it_rain_commands.is_empty() {
            println!("{}\n", command_error_msg);
            println!("At least one 'make-it-rain' entry is required\n");
            return;
        }
        println!();

        // Determine UTXO properties required for the test
        let (utxos_required, minumum_value_required) = {
            let (mut number, mut value) = (0.0, 0);
            for command in make_it_rain_commands.clone() {
                let (number_of_txs, start_amount, amount_inc) = match get_make_it_rain_tx_values(command) {
                    Some(v) => {
                        if v.err_msg != "" {
                            println!("\n{}", command_error_msg);
                            println!("\n{}\n", v.err_msg);
                            return;
                        }
                        (v.number_of_txs, v.start_amount, v.amount_inc)
                    },
                    None => {
                        println!("Cannot process the 'make-it-rain' command");
                        return;
                    },
                };
                number += number_of_txs as f64;
                value = std::cmp::max(
                    value,
                    (start_amount + MicroTari::from(number_of_txs as u64 * amount_inc.0) + MicroTari::from(825)).0,
                );
            }
            (number as usize, value as usize)
        };

        // Start the test
        let node_service = self.node_service.clone();
        let wallet_output_service = match self.wallet_output_service.clone() {
            Some(v) => v,
            _ => {
                println!("Error: Problem with OutputManagerHandle");
                return;
            },
        };
        let wallet_transaction_service = match self.wallet_transaction_service.clone() {
            Some(v) => v,
            _ => {
                println!("Error: Problem with TransactionServiceHandle");
                return;
            },
        };
        let executor = self.executor.clone();
        self.executor.spawn(async move {
            // Count number of spendable UTXOs available for the test
            let utxo_start_count = match get_number_of_spendable_utxos(
                &minumum_value_required,
                &mut node_service.clone(),
                &mut wallet_output_service.clone(),
            )
            .await
            {
                Some(v) => v,
                _ => {
                    println!("Cannot query the number of UTXOs");
                    return;
                },
            };
            let utxos_to_be_created = std::cmp::max(utxos_required as i32 - utxo_start_count as i32, 0) as usize;
            println!(
                "The test requires {} UTXOs, minimum value of {} each (average fee included); our current wallet has \
                 {} UTXOs that are adequate.\n",
                &utxos_required, &minumum_value_required, &utxo_start_count
            );

            // Perform coin-split only if requested, otherwise test spendable UTXOs may become encumbered
            let mut utxo_count = utxo_start_count;
            if utxos_to_be_created > 0 {
                println!(
                    "Command: coin-split {} {}\n",
                    minumum_value_required, utxos_to_be_created
                );

                // Count number of UTXOs available for the coin split
                let utxos_available_for_split = match get_number_of_spendable_utxos(
                    &(minumum_value_required * 100),
                    &mut node_service.clone(),
                    &mut wallet_output_service.clone(),
                )
                .await
                {
                    Some(v) => v,
                    _ => {
                        println!("Cannot query the number of UTXOs");
                        return;
                    },
                };
                let utxos_to_be_split = &utxos_to_be_created.div_euclid(99) + 1;
                let utxos_that_can_be_created = match utxos_available_for_split < utxos_to_be_split {
                    true => utxos_available_for_split * 100,
                    false => utxos_to_be_created,
                };
                println!(
                    "  - UTXOs that can be created {}, UTXOs to be split {}, UTXOs that can be split {}\n",
                    utxos_that_can_be_created, utxos_to_be_split, utxos_available_for_split,
                );

                if utxos_available_for_split > 0 {
                    // Perform requested coin split
                    for _ in 0..utxos_that_can_be_created.div_euclid(99) {
                        let args = &minumum_value_required.to_string().add(" 99");
                        println!("coin-split {}", args);
                        coin_split(
                            &mut wallet_output_service.clone(),
                            &mut wallet_transaction_service.clone(),
                            MicroTari::from(minumum_value_required as u64),
                            99,
                        )
                        .await;
                    }
                    if utxos_that_can_be_created.rem_euclid(99) > 0 {
                        let args = &minumum_value_required
                            .to_string()
                            .add(" ")
                            .add(&utxos_that_can_be_created.rem_euclid(99).to_string());
                        println!("coin-split {}", args);
                        coin_split(
                            &mut wallet_output_service.clone(),
                            &mut wallet_transaction_service.clone(),
                            MicroTari::from(minumum_value_required as u64),
                            utxos_that_can_be_created.rem_euclid(99) as usize,
                        )
                        .await;
                    }
                    println!();

                    // Wait for a sufficient number of UTXOs to be created
                    let mut count = 1usize;
                    loop {
                        tokio::time::delay_for(Duration::from_secs(60)).await;
                        // Count number of spendable UTXOs available for the test
                        utxo_count = match get_number_of_spendable_utxos(
                            &minumum_value_required,
                            &mut node_service.clone(),
                            &mut wallet_output_service.clone(),
                        )
                        .await
                        {
                            Some(v) => v,
                            _ => {
                                println!("Cannot query the number of UTXOs");
                                return;
                            },
                        };
                        if utxo_count >= utxos_required {
                            println!("We have created enough UTXOs, initiating the stress test.\n");
                            break;
                        } else {
                            println!(
                                "We still need {} UTXOs, waiting ({}) for them to be created... (current count {}, \
                                 start count {})",
                                std::cmp::max(utxos_required as i32 - utxo_count as i32, 0) as usize,
                                count,
                                utxo_count,
                                utxo_start_count,
                            );
                        }
                        if count >= 60 {
                            println!("Stress test timed out waiting for UTXOs to be created. \nPlease try again.\n",);
                            return;
                        }
                        if utxo_count >= utxo_start_count + utxos_that_can_be_created {
                            println!(
                                "Cannot perform stress test; we could not create enough UTXOs.\nPlease try again.\n",
                            );
                            return;
                        }

                        count += 1;
                    }
                }
            }

            if utxo_count < utxos_required {
                println!(
                    "Cannot perform stress test; we still need {} adequate UTXOs.\nPlease try again.\n",
                    std::cmp::max(utxos_required as i32 - utxo_count as i32, 0) as usize
                );
                return;
            }

            // Initiate make-it-rain
            for command in make_it_rain_commands {
                println!("Command: make-it-rain {}", command.iter().join(" "));
                // [Txs/s] [duration (s)] [start amount (uT)] [increment (uT)/Tx] [start time (UTC) / 'now'] [public key
                // or emoji id to send to] [message]
                let inputs = match get_make_it_rain_tx_values(command) {
                    Some(v) => {
                        if v.err_msg != "" {
                            println!("\n{}", command_error_msg);
                            println!("\n{}\n", v.err_msg);
                            return;
                        };
                        v
                    },
                    None => {
                        println!("Cannot process the 'make-it-rain' command");
                        return;
                    },
                };

                let executor_clone = executor.clone();
                let mut wallet_transaction_service_clone = wallet_transaction_service.clone();
                executor.spawn(async move {
                    make_it_rain(
                        &mut wallet_transaction_service_clone,
                        executor_clone,
                        inputs.tx_per_s,
                        inputs.number_of_txs,
                        inputs.start_amount,
                        inputs.amount_inc,
                        inputs.time_utc_start,
                        inputs.dest_pubkey.clone(),
                        inputs.msg.clone(),
                    )
                    .await;
                });
            }
        });

        println!();
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

        let executor = self.executor.clone();
        let mut wallet_transaction_service = match self.wallet_transaction_service.clone() {
            Some(v) => v,
            _ => {
                println!("Error: Problem with TransactionServiceHandle");
                return;
            },
        };
        self.executor.spawn(async move {
            make_it_rain(
                &mut wallet_transaction_service,
                executor,
                tx_per_s,
                number_of_txs,
                start_amount,
                amount_inc,
                time_utc_start,
                dest_pubkey.clone(),
                msg.clone(),
            )
            .await;
        });
    }
}

/// Function to process the send transaction command
async fn send_tari(
    amount: MicroTari,
    dest_pubkey: tari_comms::types::CommsPublicKey,
    msg: String,
    mut wallet_transaction_service: TransactionServiceHandle,
)
{
    let fee_per_gram = 25 * uT; // TODO: use configured fee per gram
    let event_stream = wallet_transaction_service.get_event_stream_fused();
    match wallet_transaction_service
        .send_transaction(dest_pubkey.clone(), amount, fee_per_gram, msg)
        .await
    {
        Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id)) => {
            println!("No peer found matching that public key. Attempting to discover the peer on the network. 🌎");
            let start = Instant::now();
            match time::timeout(
                Duration::from_secs(120),
                utils::wait_for_discovery_transaction_event(event_stream, tx_id),
            )
            .await
            {
                Ok(true) => {
                    println!(
                        "Discovery succeeded for peer {} after {}ms",
                        dest_pubkey,
                        start.elapsed().as_millis()
                    );
                    debug!(
                        target: LOG_TARGET,
                        "Discovery succeeded for peer {} after {}ms",
                        dest_pubkey,
                        start.elapsed().as_millis()
                    );
                },
                Ok(false) => {
                    println!(
                        "Discovery failed for peer {} after {}ms",
                        dest_pubkey,
                        start.elapsed().as_millis()
                    );
                    println!("The peer may be offline. Please try again later.");

                    debug!(
                        target: LOG_TARGET,
                        "Discovery failed for peer {} after {}ms",
                        dest_pubkey,
                        start.elapsed().as_millis()
                    );
                },
                Err(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Discovery timed out before the node was discovered."
                    );
                    println!("Discovery timed out before the node was discovered.");
                    println!("The peer may be offline. Please try again later.");
                },
            }
        },
        Err(TransactionServiceError::OutputManagerError(OutputManagerError::NotEnoughFunds)) => {
            println!("Not enough funds to fulfill the transaction.");
        },
        Err(e) => {
            println!("Something went wrong sending funds");
            println!("{:?}", e);
            warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
        },
        Ok(_) => println!("Sending {} Tari to {} ", amount, dest_pubkey),
    };
}

#[allow(clippy::too_many_arguments)]
async fn make_it_rain(
    transaction_service: &mut TransactionServiceHandle,
    executor: runtime::Handle,
    tx_per_s: f64,
    number_of_txs: usize,
    start_amount: MicroTari,
    amount_inc: MicroTari,
    time_utc_start: DateTime<Utc>,
    dest_pubkey: CommsPublicKey,
    msg: String,
)
{
    // Ensure a valid connection is available by sending a pilot transaction. This is intended to be
    // a blocking operation before the test starts.
    let dest_pubkey_hex = dest_pubkey.clone().to_hex();
    let event_stream = transaction_service.get_event_stream_fused();
    let fee_per_gram = 25 * uT; // TODO: use configured fee per gram
    let tx_id = match transaction_service
        .send_transaction(dest_pubkey.clone(), 10000 * uT, fee_per_gram, msg.clone())
        .await
    {
        Ok(tx_id) => tx_id,
        _ => {
            println!(
                "💀 Problem sending pilot transaction to `{}`, cannot perform 'make-it-rain' test",
                &dest_pubkey_hex
            );
            return;
        },
    };
    match time::timeout(
        Duration::from_secs(120),
        utils::wait_for_discovery_transaction_event(event_stream, tx_id),
    )
    .await
    {
        Ok(true) => {
            // Wait until specified test start time
            let millis_to_wait = (time_utc_start - Utc::now()).num_milliseconds();
            println!(
                "`make-it-rain` to peer '{}' scheduled to start at {}: msg \"{}\"",
                &dest_pubkey_hex, time_utc_start, &msg
            );
            if millis_to_wait > 0 {
                tokio::time::delay_for(Duration::from_millis(millis_to_wait as u64)).await;
            }

            // Send all the transactions
            let start = Utc::now();
            for i in 0..number_of_txs {
                // Manage Tx rate
                let millis_actual = (Utc::now() - start).num_milliseconds();
                let millis_target = (i as f64 / (tx_per_s / 1000.0)) as i64;
                if millis_target - millis_actual > 0 {
                    // Maximum delay between Txs set to 120 s
                    tokio::time::delay_for(Duration::from_millis(
                        (millis_target - millis_actual).min(120_000i64) as u64
                    ))
                    .await;
                }
                // Send Tx
                let transaction_service = transaction_service.clone();
                let dest_pubkey = dest_pubkey.clone();
                let msg = msg.clone();
                executor.spawn(async move {
                    send_tari(
                        start_amount + amount_inc * (i as u64),
                        dest_pubkey,
                        msg,
                        transaction_service,
                    )
                    .await;
                });
            }
            println!(
                "`make-it-rain` to peer '{}' concluded at {}: msg \"{}\"",
                &dest_pubkey_hex,
                Utc::now(),
                &msg
            );
        },
        _ => {
            println!(
                "💀 Pilot transaction to `{}` timed out, cannot perform 'make-it-rain' test",
                &dest_pubkey_hex
            );
        },
    }
}

async fn coin_split(
    output_manager: &mut OutputManagerHandle,
    transaction_service: &mut TransactionServiceHandle,
    amount_per_split: MicroTari,
    split_count: usize,
)
{
    let fee_per_gram = 25 * uT; // TODO: use configured fee per gram
    match output_manager
        .create_coin_split(amount_per_split, split_count, fee_per_gram, None)
        .await
    {
        Ok((tx_id, tx, fee, amount)) => {
            match transaction_service
                .submit_transaction(tx_id, tx, fee, amount, "Coin split".into())
                .await
            {
                Ok(_) => println!("Coin split transaction created with tx_id:\n{}", tx_id),
                Err(e) => {
                    println!("Something went wrong creating a coin split transaction");
                    println!("{:?}", e);
                    warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
                },
            };
        },
        Err(e) => {
            println!("Something went wrong creating a coin split transaction");
            println!("{:?}", e);
            warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
        },
    };
}

// Function to delimit arguments using spaces and pairs of quotation marks, which may include spaces
fn delimit_command_string(command_str: &str) -> Vec<String> {
    // Delimit arguments using spaces and pairs of quotation marks, which may include spaces
    let arg_temp = command_str.trim().to_string();
    let re = Regex::new(r#"[^\s"]+|"(?:\\"|[^"])+""#).unwrap();
    let arg_temp_vec: Vec<&str> = re.find_iter(&arg_temp).map(|mat| mat.as_str()).collect();
    // Remove quotation marks left behind by `Regex` - it does not support look ahead and look behind
    let mut del_arg_vec = Vec::new();
    for arg in arg_temp_vec.iter().skip(1) {
        del_arg_vec.push(str::replace(arg, "\"", ""));
    }
    del_arg_vec
}

// Function to count the number of spendable UTXOs above a certain value
async fn get_number_of_spendable_utxos(
    threshold: &usize,
    node_service: &mut LocalNodeCommsInterface,
    wallet_output_service: &mut OutputManagerHandle,
) -> Option<usize>
{
    match node_service.get_metadata().await {
        Ok(data) => {
            let current_height = data.height_of_longest_chain.unwrap() as i64;
            match wallet_output_service.get_unspent_outputs().await {
                Ok(unspent_outputs) => {
                    let mut number = 0usize;
                    if !unspent_outputs.is_empty() {
                        for uo in unspent_outputs.iter() {
                            let mature_in = std::cmp::max(uo.features.maturity as i64 - current_height, 0);
                            if mature_in == 0 && uo.value.0 >= *threshold as u64 {
                                number += 1;
                            }
                        }
                    }
                    Some(number)
                },
                _ => None,
            }
        },
        _ => None,
    }
}

// Function to get make-it-rain transaction values
fn get_make_it_rain_tx_values(command_arg: Vec<String>) -> Option<MakeItRainInputs> {
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
struct MakeItRainInputs {
    tx_per_s: f64,
    number_of_txs: usize,
    start_amount: MicroTari,
    amount_inc: MicroTari,
    time_utc_start: DateTime<Utc>,
    dest_pubkey: CommsPublicKey,
    msg: String,
    err_msg: String,
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
