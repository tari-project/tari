// Copyright 2020. The Tari Project
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
    parser::{get_make_it_rain_tx_values, STRESS_TEST_USAGE},
    table::Table,
    utils,
    utils::{format_duration_basic, format_naive_datetime},
};
use chrono::{DateTime, Utc};
use log::*;
use qrcode::{render::unicode, QrCode};
use regex::Regex;
use std::{
    fs,
    io::{self, Write},
    ops::Add,
    path::PathBuf,
    string::ToString,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tari_common::GlobalConfig;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerManager, PeerManagerError, PeerQuery},
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
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    mempool::service::LocalMempoolService,
    mining::MinerInstruction,
    tari_utilities::{hex::Hex, message_format::MessageFormat, Hashable},
    transactions::{
        tari_amount::{uT, MicroTari},
        transaction::OutputFeatures,
        types::{Commitment, PublicKey, Signature},
    },
};
use tari_crypto::ristretto::{pedersen::PedersenCommitmentFactory, RistrettoPublicKey};
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
// Import the auto-generated const values from the Manifest and Git

include!(concat!(env!("OUT_DIR"), "/consts.rs"));
pub struct CommandHandler {
    executor: runtime::Handle,
    blockchain_db: AsyncBlockchainDb<LMDBDatabase>,
    discovery_service: DhtDiscoveryRequester,
    base_node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connectivity: ConnectivityRequester,
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

impl CommandHandler {
    pub fn new(executor: runtime::Handle, ctx: &BaseNodeContext, config: &GlobalConfig) -> Self {
        CommandHandler {
            executor,
            blockchain_db: ctx.blockchain_db().into(),
            discovery_service: ctx.base_node_dht().discovery_service_requester(),
            base_node_identity: ctx.base_node_identity(),
            peer_manager: ctx.base_node_comms().peer_manager(),
            connectivity: ctx.base_node_comms().connectivity(),
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

    pub fn status(&self) {
        let mut channel = self.state_machine_info.clone();
        let mut node = self.node_service.clone();
        let mut mempool = self.mempool_service.clone();
        let peer_manager = self.peer_manager.clone();
        let mut connectivity = self.connectivity.clone();
        let (mining_status, hash_rate) = if self.wallet_output_service.is_some() {
            let hashrate = self.miner_hashrate.load(Ordering::SeqCst);
            let total_hashrate = (self.miner_thread_count * hashrate) as f64 / 1_000_000.0;
            (
                if self.mining_status.load(Ordering::SeqCst) {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                },
                total_hashrate.to_string(),
            )
        } else {
            ("DISABLED".to_string(), "0".to_string())
        };
        self.executor.spawn(async move {
            let state = channel.recv().await.unwrap();
            let metadata = node.get_metadata().await.unwrap();
            let last_header = node
                .get_headers(vec![metadata.height_of_longest_chain()])
                .await
                .unwrap()
                .pop()
                .unwrap();
            let last_block_time: DateTime<Utc> = last_header.timestamp.into();
            let mempool_stats = mempool.get_mempool_stats().await.unwrap();
            let banned_peers = banned_peers(&peer_manager).await.unwrap();
            let conns = connectivity.get_active_connections().await.unwrap();
            println!(
                "{}: State: {}, Tip: {} ({}), Mempool: {} tx, Mining (H/R): {} ({} MH/s), Connections: {}, Banned: {}",
                Utc::now().format("%H:%M"),
                state.state_info.short_desc(),
                metadata.height_of_longest_chain(),
                last_block_time.to_rfc2822(),
                mempool_stats.total_txs,
                mining_status,
                hash_rate,
                conns.len(),
                banned_peers.len()
            );
        });
    }

    /// Function to process the get-state-info command
    pub fn state_info(&self) {
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

    /// Function to process the get-balance command
    pub fn get_balance(&self) {
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
    pub fn print_version(&self) {
        println!("Version: {}", VERSION);
        println!("Author: {}", AUTHOR);
    }

    pub fn list_unspent_outputs(&self) {
        if let Some(mut handler2) = self.wallet_output_service.clone() {
            let mut handler1 = self.node_service.clone();

            self.executor.spawn(async move {
                let current_height = match handler1.get_metadata().await {
                    Err(err) => {
                        println!("Failed to retrieve chain metadata: {:?}", err);
                        warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err);
                        return;
                    },
                    Ok(data) => data.height_of_longest_chain() as i64,
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

    pub fn list_transactions(&self) {
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

    pub fn list_completed_transactions(&self, n: usize, m: Option<usize>) {
        if let Some(mut transactions) = self.wallet_transaction_service.clone() {
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

    pub fn cancel_transaction(&self, tx_id: u64) {
        if let Some(mut transactions) = self.wallet_transaction_service.clone() {
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

    pub fn get_chain_meta(&self) {
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

    pub fn get_block(&self, height: u64, format: Format) {
        let blockchain = self.blockchain_db.clone();
        self.executor.spawn(async move {
            match blockchain.fetch_blocks(height..=height).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {}", err);
                    warn!(target: LOG_TARGET, "{}", err);
                    return;
                },
                Ok(mut data) => match (data.pop(), format) {
                    (Some(block), Format::Text) => println!("{}", block),
                    (Some(block), Format::Json) => println!(
                        "{}",
                        block.to_json().unwrap_or_else(|_| "Error deserializing block".into())
                    ),
                    (None, _) => println!("Block not found at height {}", height),
                },
            };
        });
    }

    pub fn search_utxo(&self, commitment: Commitment) {
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.fetch_blocks_with_utxos(vec![commitment.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: utxo found, but lock not found for utxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
    }

    pub fn search_stxo(&self, commitment: Commitment) {
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
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: stxo found, but block not found for stxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
    }

    pub fn search_kernel(&self, excess_sig: Signature) {
        let mut handler = self.node_service.clone();
        let hex_sig = excess_sig.get_signature().to_hex();
        self.executor.spawn(async move {
            match handler.get_blocks_with_kernels(vec![excess_sig]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: kernel found, but block not found for kernel signature {}",
                        hex_sig
                    ),
                },
            };
        });
    }

    /// Function to process the get-mempool-stats command
    pub fn get_mempool_stats(&self) {
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
    pub fn get_mempool_state(&self) {
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

    pub fn discover_peer(&self, dest_pubkey: Box<RistrettoPublicKey>) {
        let mut dht = self.discovery_service.clone();

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

    pub fn get_peer(&self, node_id: NodeId) {
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            match peer_manager.find_by_node_id(&node_id).await {
                Ok(peer) => {
                    let eid = EmojiId::from_pubkey(&peer.public_key);
                    println!("Emoji ID: {}", eid);
                    println!("Public Key: {}", peer.public_key);
                    println!("NodeId: {}", peer.node_id);
                    println!("Addresses:");
                    peer.addresses.iter().for_each(|a| {
                        println!("- {}", a);
                    });
                    println!("User agent: {}", peer.user_agent);
                    println!("Features: {:?}", peer.features);
                    println!("Supported protocols:");
                    peer.supported_protocols.iter().for_each(|p| {
                        println!("- {}", String::from_utf8_lossy(p));
                    });
                    if let Some(dt) = peer.banned_until() {
                        println!("Banned until {}, reason: {}", dt, peer.banned_reason);
                    }
                    if let Some(dt) = peer.last_seen() {
                        println!("Last seen: {}", dt);
                    }
                },
                Err(err) => {
                    println!("{}", err);
                },
            }
        });
    }

    pub fn list_peers(&self, filter: Option<String>) {
        let peer_manager = self.peer_manager.clone();
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

    pub fn dial_peer(&self, dest_node_id: NodeId) {
        let mut connectivity = self.connectivity.clone();

        self.executor.spawn(async move {
            let start = Instant::now();
            println!("☎️  Dialing peer...");

            match connectivity.dial_peer(dest_node_id).await {
                Ok(p) => {
                    println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
                    println!("Connection: {}", p);
                },
                Err(err) => {
                    println!("📞  Dial failed: {}", err);
                },
            }
        });
    }

    pub fn ban_peer(&self, node_id: NodeId, duration: Duration, must_ban: bool) {
        if let Some(wni) = self.wallet_node_identity.clone() {
            if wni.node_id() == &node_id {
                println!("Cannot ban our own wallet");
                return;
            }
        }
        if self.base_node_identity.node_id() == &node_id {
            println!("Cannot ban our own node");
            return;
        }

        let mut connectivity = self.connectivity.clone();
        let wallet_connectivity = self.wallet_connectivity.clone();
        let peer_manager = self.peer_manager.clone();
        let wallet_peer_manager = self.wallet_peer_manager.clone();

        self.executor.spawn(async move {
            if must_ban {
                match connectivity
                    .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
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
                        .ban_peer_until(node_id, duration, "UI manual ban".to_string())
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

    pub fn unban_all_peers(&self) {
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

    pub fn list_banned_peers(&self) {
        let peer_manager = self.peer_manager.clone();
        let wallet_peer_manager = self.wallet_peer_manager.clone();
        self.executor.spawn(async move {
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
    pub fn list_connections(&self) {
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
                        "Chain Height",
                    ]);
                    for conn in conns {
                        let peer = peer_manager
                            .find_by_node_id(conn.peer_node_id())
                            .await
                            .expect("Unexpected peer database error or peer not found");

                        let chain_height = if let Some(metadata) = peer
                            .get_metadata(1)
                            .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                        {
                            Some(format!("Height = #{}", metadata.metadata.height_of_longest_chain()))
                        } else {
                            None
                        };

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
                                .unwrap(),
                            chain_height.unwrap_or_default(),
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

    pub fn reset_offline_peers(&self) {
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
    pub fn toggle_mining(&self) {
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
    pub fn get_mining_state(&self) {
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

    pub fn list_headers(&self, start: u64, end: Option<u64>) {
        let blockchain_db = self.blockchain_db.clone();
        self.executor.spawn(async move {
            let headers = match Self::get_headers(&blockchain_db, start, end).await {
                Ok(h) if h.is_empty() => {
                    println!("No headers found");
                    return;
                },
                Ok(h) => h,
                Err(err) => {
                    println!("Failed to retrieve headers: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
            };

            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
        });
    }

    /// Function to process the get-headers command
    async fn get_headers(
        blockchain_db: &AsyncBlockchainDb<LMDBDatabase>,
        start: u64,
        end: Option<u64>,
    ) -> Result<Vec<BlockHeader>, anyhow::Error>
    {
        match end {
            Some(end) => blockchain_db.fetch_headers(start..=end).await.map_err(Into::into),
            None => {
                let tip = blockchain_db.fetch_tip_header().await?.height();
                blockchain_db
                    .fetch_headers((tip.saturating_sub(start) + 1)..)
                    .await
                    .map_err(Into::into)
            },
        }
    }

    pub fn calc_timing(&self, start: u64, end: Option<u64>) {
        let blockchain_db = self.blockchain_db.clone();
        self.executor.spawn(async move {
            let headers = match Self::get_headers(&blockchain_db, start, end).await {
                Ok(h) if h.is_empty() => {
                    println!("No headers found");
                    return;
                },
                Ok(h) => h.into_iter().rev().collect::<Vec<_>>(),
                Err(err) => {
                    println!("Failed to retrieve headers: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
            };

            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            println!(
                "Timing for blocks #{} - #{}",
                headers.first().unwrap().height,
                headers.last().unwrap().height
            );
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        });
    }

    /// Function to process the check-db command
    pub fn check_db(&self) {
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain();
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

    #[allow(deprecated)]
    pub fn period_stats(&self, period_end: u64, mut period_ticker_end: u64, period: u64) {
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain();
            // Currently gets the stats for: tx count, hash rate estimation, target difficulty, solvetime.
            let mut results: Vec<(usize, f64, u64, u64, usize)> = Vec::new();

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
                        Some(historical_block) => historical_block,
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
                        Some(historical_block) => historical_block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                height -= 1;
                if block.block().header.timestamp.as_u64() > period_ticker_end {
                    print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
                    continue;
                };
                while block.block().header.timestamp.as_u64() < period_ticker_start {
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
                period_tx_count += block.block().body.kernels().len() - 1;
                period_block_count += 1;
                let st = if prev_block.block().header.timestamp.as_u64() >= block.block().header.timestamp.as_u64() {
                    1.0
                } else {
                    (block.block().header.timestamp.as_u64() - prev_block.block().header.timestamp.as_u64()) as f64
                };
                let diff = block.accumulated_data.target_difficulty.as_u64();
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
    pub fn whoami(&self) {
        if let Some(wallet_node_identity) = self.wallet_node_identity.clone() {
            println!("======== Wallet ==========");
            println!("{}", wallet_node_identity);
            let emoji_id = EmojiId::from_pubkey(&wallet_node_identity.public_key());
            println!("Emoji ID: {}", emoji_id);
            println!();
            // TODO: Pass the network in as a var
            let qr_link = format!("tari://stibbons/pubkey/{}", &wallet_node_identity.public_key().to_hex());
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

    pub fn coin_split(&self, amount_per_split: MicroTari, split_count: usize) {
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

    pub fn send_tari(&self, amount: MicroTari, dest_pubkey: PublicKey, message: String) {
        if let Some(wallet_transaction_service) = self.wallet_transaction_service.clone() {
            self.executor.spawn(async move {
                send_tari(amount, dest_pubkey.clone(), message.clone(), wallet_transaction_service).await;
            });
        } else {
            println!("Cannot complete command, Wallet is disabled");
        }
    }

    /// Function to process the stress test transaction function
    pub fn stress_test(&self, command_arg: Vec<String>) {
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
                println!("Command: make-it-rain {}", command.join(" "));
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

    #[allow(clippy::too_many_arguments)]
    pub fn make_it_rain(
        &self,
        tx_per_s: f64,
        number_of_txs: usize,
        start_amount: MicroTari,
        amount_inc: MicroTari,
        time_utc_start: DateTime<Utc>,
        dest_pubkey: CommsPublicKey,
        msg: String,
    )
    {
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

async fn banned_peers(pm: &PeerManager) -> Result<Vec<Peer>, PeerManagerError> {
    let query = PeerQuery::new().select_where(|p| p.is_banned());
    pm.perform_query(query).await
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

pub enum Format {
    Json,
    Text,
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

// Function to count the number of spendable UTXOs above a certain value
async fn get_number_of_spendable_utxos(
    threshold: &usize,
    node_service: &mut LocalNodeCommsInterface,
    wallet_output_service: &mut OutputManagerHandle,
) -> Option<usize>
{
    match node_service.get_metadata().await {
        Ok(data) => {
            let current_height = data.height_of_longest_chain() as i64;
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

// Function to delimit arguments using spaces and pairs of quotation marks, which may include spaces
pub fn delimit_command_string(command_str: &str) -> Vec<String> {
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
