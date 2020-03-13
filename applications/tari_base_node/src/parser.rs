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
use crate::builder::NodeContainer;
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
    tari_utilities::hex::Hex,
    transactions::tari_amount::{uT, MicroTari},
};
use tari_wallet::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::handle::TransactionServiceHandle,
};
use tokio::runtime;

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
    Whoami,
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
        }
    }

    /// This will parse the provided command and execute the task
    pub fn handle_command(&mut self, command_str: &str) {
        let commands: Vec<&str> = command_str.split(' ').collect();
        let command = BaseNodeCommand::from_str(commands[0]);
        if command.is_err() {
            println!("{} is not a valid command, please enter a valid command", command_str);
            println!("Enter help or press tab for available commands");
            return;
        }
        let command = command.unwrap();
        let help_command = if commands.len() == 2 {
            Some(BaseNodeCommand::from_str(commands[1]).unwrap_or(BaseNodeCommand::Help))
        } else {
            None
        };
        if help_command != Some(BaseNodeCommand::Help) {
            return self.process_command(command, commands);
        }

        use BaseNodeCommand::*;
        match command {
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
                println!("send_tari [amount of tari to send] [public key to send to]");
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

    // Function to process commands
    fn process_command(&mut self, command: BaseNodeCommand, command_arg: Vec<&str>) {
        use BaseNodeCommand::*;
        match command {
            Help => {
                println!("Available commands are: ");
                let joined = self.commands.join(", ");
                println!("{}", joined);
            },
            GetBalance => {
                self.process_get_balance();
            },
            SendTari => {
                self.process_send_tari(command_arg);
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

    fn process_list_peers(&self) {
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            match peer_manager.flood_peers().await {
                Ok(peers) => {
                    println!(
                        "{}",
                        peers
                            .into_iter()
                            .fold(String::new(), |acc, p| { format!("{}\n{}", acc, p) })
                    );
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
                    println!(
                        "{}",
                        conns
                            .into_iter()
                            .fold(String::new(), |acc, p| { format!("{}\n{}", acc, p) })
                    );
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not list connections: {}", e.to_string());
                    return;
                },
            }
        });
    }

    fn process_whoami(&self) {
        println!("{}", self.node_identity);
    }

    // Function to process  the send transaction function
    fn process_send_tari(&mut self, command_arg: Vec<&str>) {
        if command_arg.len() != 3 {
            println!("Command entered incorrectly, please use the following format: ");
            println!("send_tari [amount of tari to send] [public key to send to]");
            return;
        }
        let amount = command_arg[1].parse::<u64>();
        if amount.is_err() {
            println!("please enter a valid amount of tari");
            return;
        }
        let amount: MicroTari = amount.unwrap().into();
        let dest_pubkey = CommsPublicKey::from_hex(command_arg[2]);
        if dest_pubkey.is_err() {
            println!("please enter a valid destination pub_key");
            return;
        }
        let dest_pubkey = dest_pubkey.unwrap();
        let fee_per_gram = 25 * uT;
        let mut handler = self.wallet_transaction_service.clone();
        self.executor.spawn(async move {
            match handler
                .send_transaction(
                    dest_pubkey.clone(),
                    amount,
                    fee_per_gram,
                    "coinbase reward from mining".into(),
                )
                .await
            {
                Err(e) => {
                    println!("Something went wrong sending funds");
                    println!("{:?}", e);
                    warn!(target: LOG_TARGET, "Error communicating with wallet: {}", e.to_string(),);
                    return;
                },
                Ok(_) => println!("Send {} Tari to {} ", amount, dest_pubkey),
            };
        });
    }
}
