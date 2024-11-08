//  Copyright 2020, The Tari Project
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

//! # Mempool Sync Protocol
//!
//! The protocol handler for the mempool is responsible for the initial sync of transactions from peers.
//! In order to prevent duplicate transactions being received from multiple peers, syncing occurs one peer at a time.
//! This node will initiate this protocol up to a configurable (`MempoolSyncConfig::num_initial_sync_peers`) number
//! of times. After that, it will only respond to sync requests from remote peers.
//!
//! ## Protocol Flow
//!
//! Alice initiates (initiator) the connection to Bob (responder).
//! As the initiator, Alice MUST send a transaction inventory
//! Bob SHOULD respond with any transactions known to him, excluding the transactions in the inventory
//! Bob MUST send a complete message (An empty `TransactionItem` or 1 byte in protobuf)
//! Bob MUST send indexes of inventory items that are not known to him
//! Alice SHOULD return the Transactions relating to those indexes
//! Alice SHOULD close the stream immediately after sending
//!
//!
//! ```text
//!  +-------+                    +-----+
//!  | Alice |                    | Bob |
//!  +-------+                    +-----+
//!  |                                |
//!  | Txn Inventory                  |
//!  |------------------------------->|
//!  |                                |
//!  |      TransactionItem(tx_b1)    |
//!  |<-------------------------------|
//!  |             ...streaming...    |
//!  |      TransactionItem(empty)    |
//!  |<-------------------------------|
//!  |  Inventory missing txn indexes |
//!  |<-------------------------------|
//!  |                                |
//!  | TransactionItem(tx_a1)         |
//!  |------------------------------->|
//!  |             ...streaming...    |
//!  | TransactionItem(empty)         |
//!  |------------------------------->|
//!  |                                |
//!  |             END                |
//! ```

use std::{
    collections::{HashMap, HashSet},
    future::poll_fn,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub use initializer::MempoolSyncInitializer;
use libp2p_substream::{ProtocolEvent, ProtocolNotification, Substream};
use log::*;
use tari_network::{
    gossipsub::{MessageAcceptance, MessageId},
    identity::PeerId,
    NetworkEvent,
    NetworkHandle,
    StreamProtocol,
};
use tari_p2p::framing;
use tokio::{
    sync::{broadcast, mpsc, Semaphore},
    task,
    time,
    time::MissedTickBehavior,
};

use crate::{
    base_node::comms_interface::{BlockEvent, BlockEventReceiver},
    chain_storage::BlockAddResult,
    mempool::{
        sync_protocol::protocol::MempoolPeerProtocol,
        transaction_id::MempoolTransactionId,
        Mempool,
        MempoolServiceConfig,
    },
};
// FIXME: fix these tests
// #[cfg(test)]
// mod test;

mod error;
mod initializer;
mod protocol;

const MAX_FRAME_SIZE: usize = 3 * 1024 * 1024; // 3 MiB
const LOG_TARGET: &str = "c::mempool::sync_protocol";

pub static MEMPOOL_SYNC_PROTOCOL: StreamProtocol = StreamProtocol::new("/tari/mempool-sync/1");

pub struct MempoolSyncProtocol {
    config: MempoolServiceConfig,
    protocol_notifier: mpsc::UnboundedReceiver<ProtocolNotification<Substream>>,
    mempool: Mempool,
    peers_attempted: HashSet<PeerId>,
    is_done: Arc<AtomicBool>,
    permits: Arc<Semaphore>,
    network: NetworkHandle,
    block_event_stream: BlockEventReceiver,
    want_list_rx: mpsc::UnboundedReceiver<NewTransactionNotification>,
    pending_request_task: Option<task::JoinHandle<()>>,
    inbound_tasks: futures_bounded::FuturesSet<()>,
}

pub struct NewTransactionNotification {
    pub propagation_source: PeerId,
    pub transaction_id: MempoolTransactionId,
    pub message_id: MessageId,
}

impl MempoolSyncProtocol {
    pub fn new(
        config: MempoolServiceConfig,
        protocol_notifier: mpsc::UnboundedReceiver<ProtocolNotification<Substream>>,
        mempool: Mempool,
        network: NetworkHandle,
        block_event_stream: BlockEventReceiver,
        want_list_rx: mpsc::UnboundedReceiver<NewTransactionNotification>,
    ) -> Self {
        Self {
            protocol_notifier,
            mempool,
            peers_attempted: HashSet::new(),
            is_done: Arc::new(AtomicBool::new(false)),
            permits: Arc::new(Semaphore::new(1)),
            network,
            block_event_stream,
            want_list_rx,
            inbound_tasks: futures_bounded::FuturesSet::new(
                Duration::from_secs(60),
                config.max_concurrent_inbound_tasks,
            ),
            pending_request_task: None,
            config,
        }
    }

    pub async fn run(mut self, mut network_events: broadcast::Receiver<NetworkEvent>) {
        info!(target: LOG_TARGET, "Mempool protocol handler has started");

        let mut want_list_buffer = Vec::new();

        let mut interval = time::interval(self.config.request_want_list_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            let mut inbound_tasks = poll_fn(|cx| self.inbound_tasks.poll_unpin(cx));
            tokio::select! {
                _ = interval.tick() => {
                    if self.is_done.load(Ordering::SeqCst) {
                        self.request_wanted_transactions(&mut want_list_buffer).await;
                    }
                }

                // Work on inbound tasks
                _ = &mut inbound_tasks => {},

                Ok(block_event) = self.block_event_stream.recv() => {
                    self.handle_block_event(&block_event).await;
                },
                Ok(event) = network_events.recv() => {
                    self.handle_network_event(event);
                },

                Some(notif) = self.protocol_notifier.recv() => {
                    self.handle_protocol_notification(notif);
                }
            }
        }
    }

    fn handle_network_event(&mut self, event: NetworkEvent) {
        #[allow(clippy::single_match)]
        match event {
            NetworkEvent::PeerIdentified {
                peer_id,
                supported_protocols,
                ..
            } => {
                if self.is_synched() || self.has_attempted_peer(peer_id) {
                    debug!(target: LOG_TARGET, "PeerConnected: Local node already synced or already attempted peer {peer_id}");
                } else if supported_protocols.iter().any(|p| *p == MEMPOOL_SYNC_PROTOCOL) {
                    debug!(target: LOG_TARGET, "PeerConnected: initiating sync with peer {peer_id}");
                    self.peers_attempted.insert(peer_id);
                    self.spawn_initiator_sync_protocol(peer_id, false);
                } else {
                    debug!(target: LOG_TARGET, "PeerConnected: remote peer {peer_id}s is not a mempool sync peer");
                }
            },
            _ => {},
        }
    }

    async fn request_wanted_transactions(&mut self, buffer: &mut Vec<NewTransactionNotification>) {
        if self.pending_request_task.as_ref().map_or(false, |t| !t.is_finished()) {
            debug!(target: LOG_TARGET, "Want list request in progress");
            return;
        }
        self.pending_request_task = None;

        if self.want_list_rx.is_empty() {
            trace!(target: LOG_TARGET, "No transactions in want list");
            return;
        }

        let remaining_buf_space = self.config.max_request_transactions.saturating_sub(buffer.len());
        if remaining_buf_space > 0 {
            // Guaranteed to add at least one item and not await indefinitely because we check is_empty() above
            self.want_list_rx.recv_many(buffer, remaining_buf_space).await;
        }

        let config = self.config.clone();
        let mempool = self.mempool.clone();
        let network = self.network.clone();
        let mut grouped = HashMap::with_capacity(buffer.len());

        buffer
            .drain(..)
            .map(|n| (n.propagation_source, n))
            .for_each(|(key, val)| {
                grouped.entry(key).or_insert_with(Vec::new).push(val);
            });

        let task = task::spawn(async move {
            for (peer_id, notifs) in grouped {
                match network
                    .open_framed_substream(peer_id, &MEMPOOL_SYNC_PROTOCOL, MAX_FRAME_SIZE)
                    .await
                {
                    Ok(framed) => {
                        let mut protocol = MempoolPeerProtocol::new(&config, framed, peer_id, &mempool);
                        let progress = protocol.request_transactions(notifs).await;
                        // Resolve each of the processed messages for this peer
                        let resolved = progress
                            .accept
                            .into_iter()
                            .map(|id| (id, MessageAcceptance::Accept))
                            .chain(progress.ignore.into_iter().map(|id| (id, MessageAcceptance::Ignore)))
                            .chain(progress.reject.into_iter().map(|id| (id, MessageAcceptance::Reject)));

                        for (id, acceptance) in resolved {
                            if let Err(err) = network
                                .report_gossip_message_validation_result(id, peer_id, acceptance)
                                .await
                            {
                                // This can only happen if the network is shutdown or crashes. no further calls will be
                                // possible so we can stop trying
                                error!(target: LOG_TARGET, "Failed to notify network: {}", err);
                                break;
                            }
                        }
                    },
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Unable to establish mempool request protocol substream to peer `{}`: {}",
                            peer_id,
                            err
                        );
                        // Fail for all peer notifications
                        for notif in notifs {
                            if let Err(err) = network
                                .report_gossip_message_validation_result(
                                    notif.message_id,
                                    notif.propagation_source,
                                    MessageAcceptance::Ignore,
                                )
                                .await
                            {
                                error!(
                                    target: LOG_TARGET,
                                    "report_gossip_message_validation_result error: {}",
                                     err
                                )
                            }
                        }
                    },
                }
            }
        });
        self.pending_request_task = Some(task);
    }

    async fn handle_block_event(&mut self, block_event: &BlockEvent) {
        use BlockEvent::{BlockSyncComplete, ValidBlockAdded};
        match block_event {
            ValidBlockAdded(_, BlockAddResult::ChainReorg { added, removed: _ }) => {
                if added.len() < self.config.block_sync_trigger {
                    return;
                }
            },
            BlockSyncComplete(tip, starting_sync_height) => {
                let added = tip.height() - starting_sync_height;
                if added < self.config.block_sync_trigger as u64 {
                    return;
                }
            },
            _ => {
                return;
            },
        }
        // we want to at least sync initial_sync_num_peers, so we reset the num_synced to 0, so it can run till
        // initial_sync_num_peers again. This is made to run as a best effort in that it will at least run the
        // initial_sync_num_peers
        let connections = match self
            .network
            .select_random_connections(self.config.initial_sync_num_peers, Default::default())
            .await
        {
            Ok(v) => {
                if v.is_empty() {
                    error!(target: LOG_TARGET, "Mempool sync could not get any peers to sync to");
                    return;
                };
                v
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Mempool sync could not get a peer to sync to: {}", e
                );
                return;
            },
        };
        for connection in connections {
            self.spawn_initiator_sync_protocol(connection.peer_id, true);
        }
    }

    fn is_synched(&self) -> bool {
        self.is_done.load(Ordering::SeqCst)
    }

    fn has_attempted_peer(&self, peer_id: PeerId) -> bool {
        self.peers_attempted.contains(&peer_id)
    }

    fn handle_protocol_notification(&mut self, notification: ProtocolNotification<Substream>) {
        match notification.event {
            ProtocolEvent::NewInboundSubstream { peer_id, substream } => {
                // TODO: we need to limit the number of sessions we handle - switch to using RPC?
                self.start_inbound_handler(peer_id, substream);
            },
        }
    }

    fn spawn_initiator_sync_protocol(&self, peer_id: PeerId, force_sync: bool) {
        let mempool = self.mempool.clone();
        let permits = self.permits.clone();
        let is_done = self.is_done.clone();
        let config = self.config.clone();
        let network = self.network.clone();
        let num_synced = self.peers_attempted.len();
        task::spawn(async move {
            // Only initiate this protocol with a single peer at a time
            let _permit = permits.acquire().await;
            if !force_sync && is_done.load(Ordering::SeqCst) {
                return;
            }
            match network
                .open_framed_substream(peer_id, &MEMPOOL_SYNC_PROTOCOL, MAX_FRAME_SIZE)
                .await
            {
                Ok(framed) => {
                    let initial_sync_num_peers = config.initial_sync_num_peers;
                    let protocol = MempoolPeerProtocol::new(&config, framed, peer_id, &mempool);
                    match protocol.start_initiator_sync().await {
                        Ok(_) => {
                            debug!(
                                target: LOG_TARGET,
                                "Mempool initiator protocol completed successfully for peer `{}`",
                                peer_id,
                            );
                            if num_synced >= initial_sync_num_peers {
                                is_done.store(true, Ordering::SeqCst);
                            }
                        },
                        Err(err) => {
                            debug!(
                                target: LOG_TARGET,
                                "Mempool initiator protocol failed for peer `{}`: {}",
                                peer_id,
                                err
                            );
                        },
                    }
                },
                Err(err) => warn!(
                    target: LOG_TARGET,
                    "Unable to establish mempool protocol substream to peer `{}`: {}",
                    peer_id,
                    err
                ),
            }
        });
    }

    fn start_inbound_handler(&mut self, peer_id: PeerId, substream: Substream) {
        let mempool = self.mempool.clone();
        let config = self.config.clone();
        let fut = async move {
            let framed = framing::canonical(substream, MAX_FRAME_SIZE);
            let mut protocol = MempoolPeerProtocol::new(&config, framed, peer_id, &mempool);
            match protocol.start_responder().await {
                Ok(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool responder protocol succeeded for peer `{}`",
                        peer_id
                    );
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool responder protocol failed for peer `{}`: {}",
                        peer_id,
                        err
                    );
                },
            }
        };
        if self.inbound_tasks.try_push(fut).is_err() {
            warn!(target: LOG_TARGET, "Rejecting inbound task for peer {peer_id} because we've reached the max_concurrent_inbound_tasks ({})", self.config.max_concurrent_inbound_tasks);
        }
    }
}
