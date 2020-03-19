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
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    hint::{Hinter, HistoryHinter},
    line_buffer::LineBuffer,
    Context,
};
use rustyline_derive::{Helper, Highlighter, Validator};
use std::{
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
    peer_manager::PeerManager,
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_core::{
    base_node::LocalNodeCommsInterface,
    tari_utilities::{hex::Hex, Hashable},
    transactions::tari_amount::{uT, MicroTari},
};
use tari_wallet::{
    output_manager_service::handle::OutputManagerHandle,
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
    SendTari,
    GetChainMetadata,
    ListPeers,
    ListConnections,
    ListHeaders,
    GetBlock,
    Whoami,
    ToggleMining,
    Quit,
    Exit,
}

/// This is used to parse commands from the user and execute them
#[derive(Helper, Validator, Highlighter)]
pub struct Parser {
    executor: runtime::Handle,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connection_manager: ConnectionManagerRequester,
    shutdown_flag: Arc<AtomicBool>,
    commands: Vec<String>,
    hinter: HistoryHinter,
    wallet_output_service: OutputManagerHandle,
    node_service: LocalNodeCommsInterface,
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
            node_identity: ctx.node_identity(),
            peer_manager: ctx.comms().peer_manager(),
            connection_manager: ctx.comms().connection_manager(),
            shutdown_flag: ctx.interrupt_flag(),
            commands: BaseNodeCommand::iter().map(|x| x.to_string()).collect(),
            hinter: HistoryHinter {},
            wallet_output_service: ctx.output_manager(),
            node_service: ctx.local_node(),
            wallet_transaction_service: ctx.wallet_transaction_service(),
            enable_miner: ctx.miner_enabled(),
        }
    }

    /// This will parse the provided command and execute the task
    pub fn handle_command(&mut self, command_str: &str) {
        let mut args = command_str.split(' ');
        let command = BaseNodeCommand::from_str(args.next().unwrap_or(&"help"));
        if command.is_err() {
            println!("{} is not a valid command, please enter a valid command", command_str);
            println!("Enter help or press tab for available commands");
            return;
        }
        let command = command.unwrap();
        self.process_command(command, args);
    }

    // Function to process commands
    fn process_command<'a, I: Iterator<Item = &'a str>>(&mut self, command: BaseNodeCommand, args: I) {
        use BaseNodeCommand::*;
        match command {
            Help => {
                self.print_help(args);
            },
            GetBalance => {
                self.process_get_balance();
            },
            SendTari => {
                self.process_send_tari(args);
            },
            GetChainMetadata => {
                self.process_get_chain_meta();
            },
            ListPeers => {
                self.process_list_peers();
            },
            ListConnections => {
                self.process_list_connections();
            },
            ListHeaders => {
                self.process_list_headers(args);
            },
            ToggleMining => {
                self.process_toggle_mining();
            },
            GetBlock => {
                self.process_get_block(args);
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
                self.shutdown_flag.store(true, Ordering::SeqCst);
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
            SendTari => {
                println!("Sends an amount of Tari to a address call this command via:");
                println!("send-tari [amount of tari to send] [public key to send to]");
            },
            GetChainMetadata => {
                println!("Gets your base node chain meta data");
            },
            ListPeers => {
                println!("Lists the peers that this node knows about");
            },
            ListConnections => {
                println!("Lists the peer connections currently held by this node");
            },
            ListHeaders => {
                println!("List the amount of headers, can be called in the following two ways: ");
                println!("list-headers [first header height] [last header height]");
                println!("list-headers [number of headers starting from the chain tip back]");
            },
            ToggleMining => {
                println!("Enable or disable the miner on this node, calling this command will toggle the state");
            },
            GetBlock => {
                println!("View a block of a height, call this command via:");
                println!("get-block [height of the block]");
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
                    warn!(target: LOG_TARGET, "Error communicating with wallet: {}", e.to_string(),);
                    return;
                },
                Ok(data) => println!("Balances:\n{}", data),
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
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
                Ok(data) => println!("Current meta data is is: {}", data),
            };
        });
    }

    fn process_get_block<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.take(4).collect::<Vec<&str>>();
        let height = if command_arg.len() == 1 {
            let height = command_arg[0].parse::<u64>();
            if height.is_err() {
                println!("Invalid number provided");
                return;
            };
            vec![height.unwrap()]
        } else {
            println!("Invalid command, please enter as follows:");
            println!("get-block [height of the block]");
            return;
        };
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks(height).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
                Ok(data) => println!("{}", data[0].block),
            };
        });
    }

    fn process_list_peers(&self) {
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            match peer_manager.flood_peers().await {
                Ok(peers) => {
                    let num_peers = peers.len();
                    println!(
                        "{}",
                        peers
                            .into_iter()
                            .fold(String::new(), |acc, p| { format!("{}\n{}", acc, p) })
                    );

                    println!("{} peer(s) known by this node", num_peers);
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not read peers: {}", e.to_string());
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
                            .fold(String::new(), |acc, p| { format!("{}\n{}", acc, p) })
                    );
                    println!("{} active connection(s)", num_connections);
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not list connections: {}", e.to_string());
                    return;
                },
            }
        });
    }

    fn process_toggle_mining(&mut self) {
        let new_state = !self.enable_miner.load(Ordering::SeqCst);
        self.enable_miner.store(new_state, Ordering::SeqCst);
        debug!("Mining enabled is now switched to {}", new_state);
        println!("Mining enabled is now switched to {}", new_state);
    }

    fn process_list_headers<'a, I: Iterator<Item = &'a str>>(&self, args: I) {
        let command_arg = args.take(4).collect::<Vec<&str>>();
        if (command_arg.is_empty()) || (command_arg.len() > 2) {
            println!("Command entered incorrectly, please use the following formats: ");
            println!("list-headers [first header height] [last header height]");
            println!("list-headers [amount of headers from top]");
            return;
        }
        let height = if command_arg.len() == 2 {
            let height = command_arg[1].parse::<u64>();
            if height.is_err() {
                println!("Invalid number provided");
                return;
            };
            Some(height.unwrap())
        } else {
            None
        };
        let start = command_arg[0].parse::<u64>();
        if start.is_err() {
            println!("Invalid number provided");
            return;
        };
        let counter = if command_arg.len() == 2 {
            let start = start.unwrap();
            let temp_height = height.clone().unwrap();
            if temp_height <= start {
                println!("start hight should be bigger than the end height");
                return;
            }
            (temp_height - start) as usize
        } else {
            start.unwrap() as usize
        };

        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
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
            let headers = match handler.get_header(headers).await {
                Err(err) => {
                    println!("Failed to retrieve headers: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
                Ok(data) => data,
            };
            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
        });
    }

    fn process_whoami(&self) {
        println!("{}", self.node_identity);
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

        let dest_pubkey = match EmojiId::str_to_pubkey(&key).or_else(|_| CommsPublicKey::from_hex(&key)) {
            Ok(v) => v,
            _ => {
                println!("Please enter a valid destination public key or emoji id");
                return;
            },
        };
        let fee_per_gram = 25 * uT;
        let mut txn_service = self.wallet_transaction_service.clone();
        self.executor.spawn(async move {
            let event_stream = txn_service.get_event_stream_fused();
            match txn_service
                .send_transaction(
                    dest_pubkey.clone(),
                    amount,
                    fee_per_gram,
                    "Coinbase reward from mining".into(),
                )
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
                Err(e) => {
                    println!("Something went wrong sending funds");
                    println!("{:?}", e);
                    warn!(target: LOG_TARGET, "Error communicating with wallet: {}", e.to_string(),);
                    return;
                },
                Ok(_) => println!("Sending {} Tari to {} ", amount, dest_pubkey),
            };
        });
    }
}
