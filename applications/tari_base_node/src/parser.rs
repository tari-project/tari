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
use crate::{builder::NodeContainer, utils};
use log::*;
use qrcode::{render::unicode, QrCode};
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    hint::{Hinter, HistoryHinter},
    line_buffer::LineBuffer,
    Context,
};
use rustyline_derive::{Helper, Highlighter, Validator};
use std::{
    io::{self, Write},
    str::FromStr,
    string::ToString,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};
use tari_comms::{
    connection_manager::ConnectionManagerRequester,
    peer_manager::{PeerFeatures, PeerManager, PeerQuery},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::{
    base_node::LocalNodeCommsInterface,
    blocks::BlockHeader,
    mempool::service::LocalMempoolService,
    tari_utilities::{hex::Hex, Hashable},
    transactions::{
        tari_amount::{uT, MicroTari},
        transaction::OutputFeatures,
    },
};
use tari_crypto::ristretto::pedersen::PedersenCommitmentFactory;
use tari_shutdown::Shutdown;
use tari_wallet::{
    output_manager_service::{error::OutputManagerError, handle::OutputManagerHandle},
    transaction_service::{error::TransactionServiceError, handle::TransactionServiceHandle},
    util::emoji::EmojiId,
};
use tokio::{runtime, time};

/// Enum representing commands used by the basenode
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum BaseNodeCommand {
    Help,
    GetBalance,
    ListUtxos,
    SendTari,
    GetChainMetadata,
    ListPeers,
    BanPeer,
    UnbanPeer,
    ListConnections,
    ListHeaders,
    CheckDb,
    CalcTiming,
    DiscoverPeer,
    GetBlock,
    GetMempoolStats,
    GetMempoolState,
    Whoami,
    ToggleMining,
    Quit,
    Exit,
}

/// This is used to parse commands from the user and execute them
#[derive(Helper, Validator, Highlighter)]
pub struct Parser {
    executor: runtime::Handle,
    wallet_node_identity: Arc<NodeIdentity>,
    discovery_service: DhtDiscoveryRequester,
    base_node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connection_manager: ConnectionManagerRequester,
    commands: Vec<String>,
    hinter: HistoryHinter,
    wallet_output_service: OutputManagerHandle,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    wallet_transaction_service: TransactionServiceHandle,
    enable_miner: Arc<AtomicBool>,
}

// This will go through all instructions and look for potential matches
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

// This allows us to make hints based on historic inputs
impl Hinter for Parser {
    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Parser {
    /// creates a new parser struct
    pub fn new(executor: runtime::Handle, ctx: &NodeContainer) -> Self {
        Parser {
            executor,
            wallet_node_identity: ctx.wallet_node_identity(),
            discovery_service: ctx.base_node_dht().discovery_service_requester(),
            base_node_identity: ctx.base_node_identity(),
            peer_manager: ctx.base_node_comms().peer_manager(),
            connection_manager: ctx.base_node_comms().connection_manager(),
            commands: BaseNodeCommand::iter().map(|x| x.to_string()).collect(),
            hinter: HistoryHinter {},
            wallet_output_service: ctx.output_manager(),
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            wallet_transaction_service: ctx.wallet_transaction_service(),
            enable_miner: ctx.miner_enabled(),
        }
    }

    /// This will parse the provided command and execute the task
    pub fn handle_command(&mut self, command_str: &str, shutdown: &mut Shutdown) {
        if command_str.trim().is_empty() {
            return;
        }
        let mut args = command_str.split_whitespace();
        let command = BaseNodeCommand::from_str(args.next().unwrap_or(&"help"));
        if command.is_err() {
            println!("{} is not a valid command, please enter a valid command", command_str);
            println!("Enter help or press tab for available commands");
            return;
        }
        let command = command.unwrap();
        self.process_command(command, args, shutdown);
    }

    // Function to process commands
    fn process_command<'a, I: Iterator<Item = &'a str>>(
        &mut self,
        command: BaseNodeCommand,
        args: I,
        shutdown: &mut Shutdown,
    )
    {
        use BaseNodeCommand::*;
        match command {
            Help => {
                self.print_help(args);
            },
            GetBalance => {
                self.process_get_balance();
            },
            ListUtxos => {
                self.process_list_unspent_outputs();
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
            CheckDb => {
                self.process_check_db();
            },
            BanPeer => {
                self.process_ban_peer(args, true);
            },
            UnbanPeer => {
                self.process_ban_peer(args, false);
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
            GetBlock => {
                self.process_get_block(args);
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

    fn print_help<'a, I: Iterator<Item = &'a str>>(&self, mut args: I) {
        let help_for = BaseNodeCommand::from_str(args.next().unwrap_or_default()).unwrap_or(BaseNodeCommand::Help);
        use BaseNodeCommand::*;
        match help_for {
            Help => {
                println!("Available commands are: ");
                let joined = self.commands.join(", ");
                println!("{}", joined);
            },
            GetBalance => {
                println!("Gets your balance");
            },
            ListUtxos => {
                println!("List your UTXOs");
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
            BanPeer => {
                println!("Bans a peer");
            },
            UnbanPeer => {
                println!("Removes the peer ban");
            },
            CheckDb => {
                println!("Checks the blockchain database for missing blocks and headers");
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
            GetBlock => {
                println!("View a block of a height, call this command via:");
                println!("get-block [height of the block]");
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

    // Function to process  the get balance command
    fn process_get_balance(&mut self) {
        let mut handler = self.wallet_output_service.clone();
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
    }

    // Function to process the list utxos command
    fn process_list_unspent_outputs(&mut self) {
        let mut handler1 = self.node_service.clone();
        let mut handler2 = self.wallet_output_service.clone();
        self.executor.spawn(async move {
            let mut current_height = 0 as i64;
            match handler1.get_metadata().await {
                Err(err) => {
                    println!("Failed to retrieve chain metadata: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err);
                    return;
                },
                Ok(data) => current_height = data.height_of_longest_chain.unwrap() as i64,
            };
            match handler2.get_unspent_outputs().await {
                Err(e) => {
                    println!("Something went wrong");
                    warn!(target: LOG_TARGET, "Error communicating with wallet: {:?}", e);
                    return;
                },
                Ok(unspent_outputs) => {
                    if unspent_outputs.len() > 0 {
                        println!(
                            "\nYou have {} UTXOs: (value, commitment, mature in ? blocks, flags)",
                            unspent_outputs.len()
                        );
                        let factory = PedersenCommitmentFactory::default();
                        for uo in unspent_outputs.iter() {
                            let mature_in = std::cmp::max(uo.features.maturity as i64 - *&current_height, 0);
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
                        println!("");
                    } else {
                        println!("\nNo valid UTXOs found at this time\n");
                    }
                },
            };
        });
    }

    // Function to process  the get chain meta data
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

    fn process_get_block<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.take(4).collect::<Vec<&str>>();
        let height = if command_arg.len() == 1 {
            match command_arg[0].parse::<u64>().ok() {
                Some(height) => height,
                None => {
                    println!("Invalid block height provided. Height must be an integer.");
                    return;
                },
            }
        } else {
            println!("Invalid command, please enter as follows:");
            println!("get-block [height of the block]");
            println!("e.g. get-block 10");
            return;
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
                Ok(mut data) => match data.pop() {
                    Some(historical_block) => println!("{}", historical_block.block),
                    None => println!("Block not found at height {}", height),
                },
            };
        });
    }

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
            match dht.discover_peer(dest_pubkey, None, NodeDestination::Unknown).await {
                Ok(p) => {
                    let end = Instant::now();
                    println!("⚡️ Discovery succeeded in {}ms!", (end - start).as_millis());
                    println!("This peer was found:");
                    println!("{}", p);
                },
                Err(err) => {
                    println!("💀 Discovery failed: '{:?}'", err);
                },
            }
        });
    }

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
                    println!(
                        "{}",
                        peers
                            .into_iter()
                            .fold(String::new(), |acc, p| format!("{}\n{}", acc, p))
                    );
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

    fn process_ban_peer<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I, is_banned: bool) {
        let peer_manager = self.peer_manager.clone();
        let mut connection_manager = self.connection_manager.clone();

        let public_key = match args.next().and_then(parse_emoji_id_or_public_key) {
            Some(v) => Box::new(v),
            None => {
                println!("Please enter a valid destination public key or emoji id");
                println!("ban-peer/unban-peer [hex public key or emoji id]");
                return;
            },
        };

        self.executor.spawn(async move {
            match peer_manager.set_banned(&public_key, is_banned).await {
                Ok(node_id) => {
                    if is_banned {
                        match connection_manager.disconnect_peer(node_id).await {
                            Ok(_) => {
                                println!("Peer was banned.");
                            },
                            Err(err) => {
                                println!(
                                    "Peer was banned but an error occurred when disconnecting them: {:?}",
                                    err
                                );
                            },
                        }
                    } else {
                        println!("Peer ban was removed.");
                    }
                },
                Err(err) => {
                    println!("Failed to ban/unban peer: {:?}", err);
                    error!(target: LOG_TARGET, "Could not ban/unban peer: {:?}", err);
                    return;
                },
            }
        });
    }

    fn process_list_connections(&self) {
        let mut connection_manager = self.connection_manager.clone();
        self.executor.spawn(async move {
            match connection_manager.get_active_connections().await {
                Ok(conns) if conns.is_empty() => {
                    println!("No active peer connections.");
                },
                Ok(conns) => {
                    let num_connections = conns.len();
                    println!(
                        "{}",
                        conns
                            .into_iter()
                            .fold(String::new(), |acc, p| format!("{}\n{}", acc, p))
                    );
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

    fn process_toggle_mining(&mut self) {
        let new_state = !self.enable_miner.load(Ordering::SeqCst);
        self.enable_miner.store(new_state, Ordering::SeqCst);
        if new_state {
            println!("Mining is ON");
        } else {
            println!("Mining is OFF");
        }
        debug!(target: LOG_TARGET, "Mining state is now switched to {}", new_state);
    }

    fn process_list_headers<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.map(|arg| arg.to_string()).take(4).collect::<Vec<String>>();
        if (command_arg.is_empty()) || (command_arg.len() > 2) {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("list-headers [first header height] [last header height]");
            println!("list-headers [amount of headers from top]");
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

    async fn get_headers(mut handler: LocalNodeCommsInterface, command_arg: Vec<String>) -> Vec<BlockHeader> {
        let height = if command_arg.len() == 2 {
            let height = command_arg[1].parse::<u64>();
            if height.is_err() {
                println!("Invalid number provided");
                return Vec::new();
            };
            Some(height.unwrap())
        } else {
            None
        };
        let start = command_arg[0].parse::<u64>();
        if start.is_err() {
            println!("Invalid number provided");
            return Vec::new();
        };
        let counter = if command_arg.len() == 2 {
            let start = start.unwrap();
            let temp_height = height.clone().unwrap();
            if temp_height <= start {
                println!("start hight should be bigger than the end height");
                return Vec::new();
            }
            (temp_height - start) as usize
        } else {
            start.unwrap() as usize
        };
        let mut height = if let Some(v) = height {
            v
        } else {
            match handler.get_metadata().await {
                Err(err) => {
                    println!("Failed to retrieve chain height: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    0
                },
                Ok(data) => data.height_of_longest_chain.unwrap_or(0),
            }
        };
        let mut headers = Vec::new();
        headers.push(height);
        while (headers.len() <= counter) && (height > 0) {
            height -= 1;
            headers.push(height);
        }
        match handler.get_headers(headers).await {
            Err(err) => {
                println!("Failed to retrieve headers: {:?}", err);
                warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                Vec::new()
            },
            Ok(data) => data,
        }
    }

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
            let (max, min, avg) = timing_stats(&headers);
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        });
    }

    fn process_check_db(&mut self) {
        // Todo, add calls to ask peers for missing data
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain.expect("Could not retrieve chain height");
            let mut missing_blocks = Vec::new();
            let mut missing_headers = Vec::new();
            print!("Searching for height: ");
            while height > 0 {
                print!("{}", height);
                io::stdout().flush().unwrap();
                let block = node.get_blocks(vec![height]).await;
                if block.is_err() {
                    // for some apparent reason this block is missing, means we have to ask for it again
                    missing_blocks.push(height);
                };
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

    fn process_whoami(&self) {
        println!("======== Wallet ==========");
        println!("{}", self.wallet_node_identity);
        let emoji_id = EmojiId::from_pubkey(&self.wallet_node_identity.public_key());
        println!("Emoji ID: {}", emoji_id);
        println!();
        // TODO: Pass the network in as a var
        let qr_link = format!(
            "tari://rincewind/pubkey/{}",
            &self.wallet_node_identity.public_key().to_hex()
        );
        let code = QrCode::new(qr_link).unwrap();
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .build();
        println!("{}", image);
        println!();
        println!("======== Base Node ==========");
        println!("{}", self.base_node_identity);
    }

    // Function to process  the send transaction function
    fn process_send_tari<'a, I: Iterator<Item = &'a str>>(&mut self, mut args: I) {
        let amount = args.next().and_then(|v| v.parse::<u64>().ok());
        if amount.is_none() {
            println!("Please enter a valid amount of tari");
            return;
        }
        let amount: MicroTari = amount.unwrap().into();

        let key = match args.next() {
            Some(k) => k.to_string(),
            None => {
                println!("Command entered incorrectly, please use the following format: ");
                println!("send_tari [amount of tari to send] [public key or emoji id to send to]");
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

        // Use the rest of the command line as my message
        let msg = args.collect::<Vec<&str>>().join(" ");

        let fee_per_gram = 25 * uT;
        let mut txn_service = self.wallet_transaction_service.clone();
        self.executor.spawn(async move {
            let event_stream = txn_service.get_event_stream_fused();
            match txn_service
                .send_transaction(dest_pubkey.clone(), amount, fee_per_gram, msg)
                .await
            {
                Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id)) => {
                    println!(
                        "No peer found matching that public key. Attempting to discover the peer on the network. 🌎"
                    );
                    let start = Instant::now();
                    match time::timeout(
                        Duration::from_secs(120),
                        utils::wait_for_discovery_transaction_event(event_stream, tx_id),
                    )
                    .await
                    {
                        Ok(true) => {
                            let end = Instant::now();
                            println!(
                                "Discovery succeeded for peer {} after {}ms",
                                dest_pubkey,
                                (end - start).as_millis()
                            );
                            debug!(
                                target: LOG_TARGET,
                                "Discovery succeeded for peer {} after {}ms",
                                dest_pubkey,
                                (end - start).as_millis()
                            );
                        },
                        Ok(false) => {
                            let end = Instant::now();
                            println!(
                                "Discovery failed for peer {} after {}ms",
                                dest_pubkey,
                                (end - start).as_millis()
                            );
                            println!("The peer may be offline. Please try again later.");

                            debug!(
                                target: LOG_TARGET,
                                "Discovery failed for peer {} after {}ms",
                                dest_pubkey,
                                (end - start).as_millis()
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
                    return;
                },
                Ok(_) => println!("Sending {} Tari to {} ", amount, dest_pubkey),
            };
        });
    }
}

fn parse_emoji_id_or_public_key(key: &str) -> Option<CommsPublicKey> {
    EmojiId::str_to_pubkey(&key.trim().replace('|', ""))
        .or_else(|_| CommsPublicKey::from_hex(key))
        .ok()
}

/// Given a slice of headers (in reverse order), calculate the maximum, minimum and average periods between them
fn timing_stats(headers: &[BlockHeader]) -> (u64, u64, f64) {
    let (max, min) = headers.windows(2).fold((0u64, std::u64::MAX), |(max, min), next| {
        let delta_t = match next[0].timestamp.checked_sub(next[1].timestamp) {
            Some(delta) => delta.as_u64(),
            None => 0u64,
        };
        let min = min.min(delta_t);
        let max = max.max(delta_t);
        (max, min)
    });
    let avg = if headers.len() >= 2 {
        let dt = headers.first().unwrap().timestamp - headers.last().unwrap().timestamp;
        let n = headers.len() - 1;
        dt.as_u64() as f64 / n as f64
    } else {
        0.0
    };
    (max, min, avg)
}

#[cfg(test)]
mod test {
    use crate::parser::timing_stats;
    use tari_core::{blocks::BlockHeader, tari_utilities::epoch_time::EpochTime};

    #[test]
    fn test_timing_stats() {
        let headers = vec![500, 350, 300, 210, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: EpochTime::from(t),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = timing_stats(&headers);
        assert_eq!(max, 150);
        assert_eq!(min, 50);
        assert_eq!(avg, 100f64);
    }

    #[test]
    fn timing_negative_blocks() {
        let headers = vec![150, 90, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: EpochTime::from(t),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 0);
        assert_eq!(avg, 25f64);
    }

    #[test]
    fn timing_empty_list() {
        let (max, min, avg) = timing_stats(&[]);
        assert_eq!(max, 0);
        assert_eq!(min, std::u64::MAX);
        assert_eq!(avg, 0f64);
    }
}
