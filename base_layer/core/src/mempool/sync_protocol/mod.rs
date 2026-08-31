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
    convert::TryFrom,
    iter,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use error::MempoolProtocolError;
use futures::{SinkExt, Stream, StreamExt, stream};
pub use initializer::MempoolSyncInitializer;
use log::*;
use prost::Message;
use tari_comms::{
    Bytes,
    PeerConnection,
    connectivity::{ConnectivityEvent, ConnectivityRequester, ConnectivitySelection},
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::{NodeId, PeerFeatures},
    protocol::{ProtocolEvent, ProtocolNotification, ProtocolNotificationRx},
};
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::{ByteArray, hex::Hex};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Semaphore,
    task::JoinSet,
    time,
};

#[cfg(feature = "metrics")]
use crate::mempool::metrics;
use crate::{
    base_node::comms_interface::{BlockEvent, BlockEventReceiver},
    chain_storage::BlockAddResult,
    mempool::{Mempool, MempoolServiceConfig, proto},
    proto as shared_proto,
};

#[cfg(test)]
mod test;

mod error;
mod initializer;

const MAX_FRAME_SIZE: usize = 3 * 1024 * 1024; // 3 MiB

/// Deadline for a single control message — the transaction inventory and the list of requested
/// indexes. Both are one bounded frame, so a short deadline is safe.
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(10);

/// Deadline for delivering one frame of the transaction stream, and for closing the substream.
///
/// The clock restarts for every frame, so this bounds how long any single transaction may take to
/// arrive. Note it is not an idle-gap bound: the deadline covers complete delivery of the frame,
/// not merely the wait for its first byte. A frame here carries an entire transaction and may be up
/// to [`MAX_FRAME_SIZE`] (3 MiB); at the ~50 KiB/s a Tor circuit can sustain, delivering one takes
/// 61s, so this is set at roughly double that rather than at the edge of it.
///
/// Because it restarts per frame it cannot bound the exchange as a whole — a peer trickling one
/// frame just inside the deadline would hold the substream open forever. [`PROTOCOL_TIMEOUT`] and
/// the item cap in `read_and_insert_transactions_until_complete` are what bound that.
const STREAM_ITEM_TIMEOUT: Duration = Duration::from_secs(120);

/// Deadline for one peer's entire mempool sync exchange.
///
/// This is the bound that actually caps how long a peer can hold the initiator permit. Initiators
/// are serialised behind a single permit, so without an aggregate deadline one slow or malicious
/// peer wedges mempool sync for the whole node no matter how tight the per-frame deadline is. A
/// sync that cannot finish inside this is not useful for initial-sync purposes; it is abandoned and
/// retried against another peer.
const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(300);

/// How long to wait, on shutdown, for aborted peer protocol tasks to actually finish.
///
/// Aborting is not enough on its own: `JoinSet::drop` requests cancellation without waiting, so the
/// `Mempool` clones those tasks hold — and through them the blockchain database and its LMDB file
/// lock — could still be alive after this service has returned.
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const LOG_TARGET: &str = "c::mempool::sync_protocol";

/// Report the outcome of a bounded `close()`. Closing is always best effort — the exchange is over
/// either way — but the reason is worth keeping, including the inner IO error.
fn log_close_outcome(result: Result<Result<(), std::io::Error>, time::error::Elapsed>) {
    match result {
        Err(_elapsed) => debug!(target: LOG_TARGET, "Timed out closing stream"),
        Ok(Err(err)) => debug!(target: LOG_TARGET, "IO error when closing stream: {err}"),
        Ok(Ok(())) => {},
    }
}

pub static MEMPOOL_SYNC_PROTOCOL: Bytes = Bytes::from_static(b"t/mempool-sync/1");

pub struct MempoolSyncProtocol<TSubstream> {
    config: MempoolServiceConfig,
    protocol_notifier: ProtocolNotificationRx<TSubstream>,
    mempool: Mempool,
    num_synched: Arc<AtomicUsize>,
    permits: Arc<Semaphore>,
    connectivity: ConnectivityRequester,
    block_event_stream: BlockEventReceiver,
    shutdown_signal: ShutdownSignal,
    /// Owns every spawned peer protocol task. Each one holds a `Mempool` clone, so they must die
    /// with this protocol: dropping the set aborts them and releases those clones. Detaching them
    /// (a bare `task::spawn`) left them alive after the node had shut down, pinning the mempool's
    /// validator and through it the blockchain database and its LMDB file lock.
    tasks: JoinSet<()>,
}

impl<TSubstream> MempoolSyncProtocol<TSubstream>
where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static
{
    pub fn new(
        config: MempoolServiceConfig,
        protocol_notifier: ProtocolNotificationRx<TSubstream>,
        mempool: Mempool,
        connectivity: ConnectivityRequester,
        block_event_stream: BlockEventReceiver,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            protocol_notifier,
            mempool,
            num_synched: Arc::new(AtomicUsize::new(0)),
            permits: Arc::new(Semaphore::new(1)),
            connectivity,
            block_event_stream,
            shutdown_signal,
            tasks: JoinSet::new(),
        }
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Mempool protocol handler has started");

        let mut connectivity_events = self.connectivity.get_event_subscription();

        // Trigger initial mempool sync with already-connected peers. When the mempool sync
        // protocol starts, the node has already completed chain sync, so PeerConnected and
        // BlockSyncComplete events have already been emitted and cannot be received by this
        // protocol's event loop. Proactively request existing connections here.
        if !self.is_synched() {
            match self
                .connectivity
                .select_connections(ConnectivitySelection::random_nodes(
                    self.config.initial_sync_num_peers,
                    vec![],
                ))
                .await
            {
                Ok(connections) => {
                    for connection in connections {
                        self.spawn_initiator_protocol(connection).await;
                    }
                },
                Err(e) => {
                    debug!(target: LOG_TARGET, "Mempool startup sync: could not get peers: {e}");
                },
            }
        }

        loop {
            tokio::select! {
                Ok(block_event) = self.block_event_stream.recv() => {
                    self.handle_block_event(&block_event).await;
                },
                Ok(event) = connectivity_events.recv() => {
                    self.handle_connectivity_event(event).await;
                },

                Some(notif) = self.protocol_notifier.recv() => {
                    self.handle_protocol_notification(notif);
                },

                // Reap finished peer protocols so the set does not grow for the life of the node.
                Some(result) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    if let Err(err) = result
                        && !err.is_cancelled()
                    {
                        warn!(target: LOG_TARGET, "Mempool peer protocol task terminated abnormally: {err}");
                    }
                },

                _ = &mut self.shutdown_signal => {
                    info!(
                        target: LOG_TARGET,
                        "Mempool protocol handler is shutting down, aborting {} peer protocol task(s)",
                        self.tasks.len()
                    );
                    break;
                }
            }
        }

        // Abort *and* wait. `JoinSet::drop` only requests cancellation, so returning here without
        // joining would leave the aborted tasks — and the `Mempool` clones they hold, and through
        // those the blockchain database and its LMDB file lock — alive for an unbounded moment
        // after this service has reported itself shut down. Deterministic release is the whole
        // point, so the wait is bounded rather than best effort.
        //
        // One residual remains and cannot be closed from here: `Mempool` operations run inside
        // `spawn_blocking`, and aborting the async task detaches the blocking closure, which holds
        // its own handle on the mempool storage until it returns. Those closures are short
        // in-memory operations under the storage lock, so the window is brief, but it is not zero.
        if time::timeout(TASK_SHUTDOWN_TIMEOUT, self.tasks.shutdown())
            .await
            .is_err()
        {
            warn!(
                target: LOG_TARGET,
                "Peer protocol tasks did not finish within {TASK_SHUTDOWN_TIMEOUT:?} of being aborted"
            );
        }
    }

    async fn handle_connectivity_event(&mut self, event: ConnectivityEvent) {
        match event {
            // If this node is connecting to a peer
            ConnectivityEvent::PeerConnected(conn) if conn.direction().is_outbound() => {
                // This protocol is only spoken between base nodes
                if !conn.peer_features().contains(PeerFeatures::COMMUNICATION_NODE) {
                    return;
                }

                if !self.is_synched() {
                    self.spawn_initiator_protocol(*conn.clone()).await;
                }
            },
            _ => {},
        }
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
                let added = tip.height().saturating_sub(*starting_sync_height);
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
        self.num_synched.store(0, Ordering::SeqCst);
        let connections = match self
            .connectivity
            .select_connections(ConnectivitySelection::random_nodes(
                self.config.initial_sync_num_peers,
                vec![],
            ))
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
                    "Mempool sync could not get a peer to sync to: {e}"
                );
                return;
            },
        };
        for connection in connections {
            self.spawn_initiator_protocol(connection).await;
        }
    }

    fn is_synched(&self) -> bool {
        self.num_synched.load(Ordering::SeqCst) >= self.config.initial_sync_num_peers
    }

    fn handle_protocol_notification(&mut self, notification: ProtocolNotification<TSubstream>) {
        match notification.event {
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                self.spawn_inbound_handler(node_id, substream);
            },
        }
    }

    async fn spawn_initiator_protocol(&mut self, mut conn: PeerConnection) {
        let mempool = self.mempool.clone();
        let permits = self.permits.clone();
        let num_synched = self.num_synched.clone();
        let config = self.config.clone();
        self.tasks.spawn(async move {
            // Only initiate this protocol with a single peer at a time
            let _permit = permits.acquire().await;
            if num_synched.load(Ordering::SeqCst) >= config.initial_sync_num_peers {
                return;
            }
            let peer = conn.peer_node_id().clone();
            // The aggregate deadline has to span opening the substream as well as the exchange.
            // Opening is itself unbounded — an unbounded request/reply to the connection worker
            // followed by a yamux `open_stream()` with no timeout of its own (only the subsequent
            // protocol negotiation is bounded) — and it runs while holding the permit, so a peer
            // with a wedged control would otherwise stall mempool sync for the whole node exactly
            // as an unbounded read used to.
            let synced = time::timeout(PROTOCOL_TIMEOUT, async {
                let framed = match conn.open_framed_substream(&MEMPOOL_SYNC_PROTOCOL, MAX_FRAME_SIZE).await {
                    Ok(framed) => framed,
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Unable to establish mempool protocol substream to peer `{}`: {}",
                            peer.short_str(),
                            err
                        );
                        return false;
                    },
                };
                match MempoolPeerProtocol::new(config, framed, peer.clone(), mempool)
                    .start_initiator()
                    .await
                {
                    Ok(_) => {
                        debug!(
                            target: LOG_TARGET,
                            "Mempool initiator protocol completed successfully for peer `{}`",
                            peer.short_str(),
                        );
                        true
                    },
                    Err(err) => {
                        debug!(
                            target: LOG_TARGET,
                            "Mempool initiator protocol failed for peer `{}`: {}",
                            peer.short_str(),
                            err
                        );
                        false
                    },
                }
            })
            .await;

            match synced {
                Ok(true) => {
                    num_synched.fetch_add(1, Ordering::SeqCst);
                },
                Ok(false) => {},
                Err(_elapsed) => {
                    warn!(
                        target: LOG_TARGET,
                        "Mempool initiator exchange with peer `{}` exceeded {PROTOCOL_TIMEOUT:?}; abandoning it",
                        peer.short_str(),
                    );
                },
            }
        });
    }

    fn spawn_inbound_handler(&mut self, node_id: NodeId, substream: TSubstream) {
        let mempool = self.mempool.clone();
        let config = self.config.clone();
        self.tasks.spawn(async move {
            let framed = framing::canonical(substream, MAX_FRAME_SIZE);
            let mut protocol = MempoolPeerProtocol::new(config, framed, node_id.clone(), mempool);
            // Aggregate deadline, as for the initiator: a responder holds no permit, but an
            // unbounded exchange still keeps a `Mempool` clone alive indefinitely.
            match time::timeout(PROTOCOL_TIMEOUT, protocol.start_responder()).await {
                Err(_elapsed) => {
                    warn!(
                        target: LOG_TARGET,
                        "Mempool responder protocol with peer `{}` exceeded {PROTOCOL_TIMEOUT:?}; abandoning it",
                        node_id.short_str()
                    );
                },
                Ok(Ok(_)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool responder protocol succeeded for peer `{}`",
                        node_id.short_str()
                    );
                },
                Ok(Err(err)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool responder protocol failed for peer `{}`: {}",
                        node_id.short_str(),
                        err
                    );
                },
            }
        });
    }
}

struct MempoolPeerProtocol<TSubstream> {
    config: MempoolServiceConfig,
    framed: CanonicalFraming<TSubstream>,
    mempool: Mempool,
    peer_node_id: NodeId,
}

impl<TSubstream> MempoolPeerProtocol<TSubstream>
where TSubstream: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(
        config: MempoolServiceConfig,
        framed: CanonicalFraming<TSubstream>,
        peer_node_id: NodeId,
        mempool: Mempool,
    ) -> Self {
        Self {
            config,
            framed,
            mempool,
            peer_node_id,
        }
    }

    pub async fn start_initiator(mut self) -> Result<(), MempoolProtocolError> {
        match self.start_initiator_inner().await {
            Ok(_) => {
                debug!(target: LOG_TARGET, "Initiator protocol complete");
                Ok(())
            },
            Err(err) => {
                // Bounded: this runs while already handling an error, often because the peer has
                // gone away, which is exactly when an unbounded flush or close never returns.
                match time::timeout(STREAM_ITEM_TIMEOUT, self.framed.flush()).await {
                    Err(_elapsed) => debug!(target: LOG_TARGET, "Timed out flushing stream"),
                    Ok(Err(err)) => debug!(target: LOG_TARGET, "IO error when flushing stream: {err}"),
                    Ok(Ok(())) => {},
                }
                log_close_outcome(time::timeout(STREAM_ITEM_TIMEOUT, self.framed.close()).await);
                Err(err)
            },
        }
    }

    async fn start_initiator_inner(&mut self) -> Result<(), MempoolProtocolError> {
        debug!(
            target: LOG_TARGET,
            "Starting initiator mempool sync for peer `{}`",
            self.peer_node_id.short_str()
        );

        let transactions = self.mempool.snapshot().await?;
        let items = transactions
            .iter()
            .take(self.config.initial_sync_max_transactions)
            .filter_map(|txn| txn.first_kernel_excess_sig())
            .map(|excess| excess.get_signature().to_vec())
            .collect();
        let inventory = proto::TransactionInventory { items };

        // Send an inventory of items currently in this node's mempool
        debug!(
            target: LOG_TARGET,
            "Sending transaction inventory containing {} item(s) to peer `{}`",
            inventory.items.len(),
            self.peer_node_id.short_str()
        );

        self.write_message(inventory).await?;

        self.read_and_insert_transactions_until_complete().await?;

        let missing_items: proto::InventoryIndexes = self.read_message().await?;
        debug!(
            target: LOG_TARGET,
            "Received {} missing transaction index(es) from peer `{}`",
            missing_items.indexes.len(),
            self.peer_node_id.short_str(),
        );
        let missing_txns = missing_items
            .indexes
            .iter()
            .filter_map(|idx| transactions.get(*idx as usize).cloned())
            .collect::<Vec<_>>();
        debug!(
            target: LOG_TARGET,
            "Sending {} missing transaction(s) to peer `{}`",
            missing_items.indexes.len(),
            self.peer_node_id.short_str(),
        );

        // If we don't have any transactions at the given indexes we still need to send back an empty if they requested
        // at least one index
        if !missing_items.indexes.is_empty() {
            self.write_transactions(missing_txns).await?;
        }

        // Close the stream after writing. The exchange is complete by this point, so a failure to
        // close is cosmetic: reporting it as an error would discard a sync that actually succeeded
        // and, in the initiator, skip `num_synched`, leaving the node spawning initiators forever.
        // It also must not fall through to the caller's flush/close retry, which would drive
        // `poll_flush` and `poll_close` on a sink whose close has already been polled.
        log_close_outcome(time::timeout(STREAM_ITEM_TIMEOUT, self.framed.close()).await);

        Ok(())
    }

    pub async fn start_responder(&mut self) -> Result<(), MempoolProtocolError> {
        match self.start_responder_inner().await {
            Ok(_) => {
                debug!(target: LOG_TARGET, "Responder protocol complete");
                Ok(())
            },
            Err(err) => {
                // Bounded: this runs while already handling an error, often because the peer has
                // gone away, which is exactly when an unbounded flush or close never returns.
                match time::timeout(STREAM_ITEM_TIMEOUT, self.framed.flush()).await {
                    Err(_elapsed) => debug!(target: LOG_TARGET, "Timed out flushing stream"),
                    Ok(Err(err)) => debug!(target: LOG_TARGET, "IO error when flushing stream: {err}"),
                    Ok(Ok(())) => {},
                }
                log_close_outcome(time::timeout(STREAM_ITEM_TIMEOUT, self.framed.close()).await);
                Err(err)
            },
        }
    }

    async fn start_responder_inner(&mut self) -> Result<(), MempoolProtocolError> {
        debug!(
            target: LOG_TARGET,
            "Starting responder mempool sync for peer `{}`",
            self.peer_node_id.short_str()
        );

        let inventory: proto::TransactionInventory = self.read_message().await?;

        debug!(
            target: LOG_TARGET,
            "Received inventory from peer `{}` containing {} item(s)",
            self.peer_node_id.short_str(),
            inventory.items.len()
        );

        let transactions = self.mempool.snapshot().await?;

        let mut duplicate_inventory_items = Vec::new();
        let (transactions, _) = transactions.into_iter().partition::<Vec<_>, _>(|transaction| {
            let excess_sig = transaction
                .first_kernel_excess_sig()
                .expect("transaction stored in mempool did not have any kernels");

            let has_item = inventory
                .items
                .iter()
                .position(|bytes| bytes.as_slice() == excess_sig.get_signature().as_bytes());

            match has_item {
                Some(pos) => {
                    duplicate_inventory_items.push(pos);
                    false
                },
                None => true,
            }
        });

        debug!(
            target: LOG_TARGET,
            "Streaming {} transaction(s) to peer `{}`",
            transactions.len(),
            self.peer_node_id.short_str()
        );

        self.write_transactions(transactions).await?;

        // Generate an index list of inventory indexes that this node does not have
        #[allow(clippy::cast_possible_truncation)]
        let missing_items = inventory
            .items
            .into_iter()
            .enumerate()
            .filter_map(|(i, _)| {
                if duplicate_inventory_items.contains(&i) {
                    None
                } else {
                    Some(i as u32)
                }
            })
            .collect::<Vec<_>>();
        debug!(
            target: LOG_TARGET,
            "Requesting {} missing transaction index(es) from peer `{}`",
            missing_items.len(),
            self.peer_node_id.short_str(),
        );

        let missing_items = proto::InventoryIndexes { indexes: missing_items };
        let num_missing_items = missing_items.indexes.len();
        self.write_message(missing_items).await?;

        if num_missing_items > 0 {
            debug!(target: LOG_TARGET, "Waiting for missing transactions");
            self.read_and_insert_transactions_until_complete().await?;
        }

        Ok(())
    }

    async fn read_and_insert_transactions_until_complete(&mut self) -> Result<(), MempoolProtocolError> {
        let mut num_recv = 0usize;
        // The sender caps what it will send (`.take(initial_sync_max_transactions)`); the receiver
        // enforces the same limit, so a peer cannot keep the exchange alive by streaming forever.
        let max_recv = self.config.initial_sync_max_transactions;
        // Bounded per item: an unbounded read here parks forever against a peer that neither sends
        // the terminator nor closes the substream — which pins this node's mempool, and with it the
        // blockchain database and its LMDB file lock, for the life of the process.
        while let Some(result) = time::timeout(STREAM_ITEM_TIMEOUT, self.framed.next())
            .await
            .map_err(|_| MempoolProtocolError::RecvTimeout)?
        {
            let bytes = result?;
            let item = proto::TransactionItem::decode(&mut bytes.freeze()).map_err(|err| {
                MempoolProtocolError::DecodeFailed {
                    source: err,
                    peer: self.peer_node_id.clone(),
                }
            })?;

            match item.transaction {
                Some(txn) => {
                    if num_recv >= max_recv {
                        return Err(MempoolProtocolError::TooManyTransactions {
                            peer: self.peer_node_id.clone(),
                            max: max_recv,
                        });
                    }
                    self.validate_and_insert_transaction(txn).await?;
                    num_recv = num_recv.saturating_add(1);
                },
                None => {
                    debug!(
                        target: LOG_TARGET,
                        "All transaction(s) (count={}) received from peer `{}`. ",
                        num_recv,
                        self.peer_node_id.short_str()
                    );
                    break;
                },
            }
        }

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_wrap)]
        #[cfg(feature = "metrics")]
        {
            let stats = self.mempool.stats().await?;
            metrics::unconfirmed_pool_size().set(stats.unconfirmed_txs as i64);
            metrics::reorg_pool_size().set(stats.reorg_txs as i64);
        }

        Ok(())
    }

    async fn validate_and_insert_transaction(
        &mut self,
        txn: shared_proto::types::Transaction,
    ) -> Result<(), MempoolProtocolError> {
        let txn = Transaction::try_from(txn).map_err(|err| MempoolProtocolError::MessageConversionFailed {
            peer: self.peer_node_id.clone(),
            message: err,
        })?;
        let excess_sig = txn
            .first_kernel_excess_sig()
            .ok_or_else(|| MempoolProtocolError::ExcessSignatureMissing(self.peer_node_id.clone()))?;
        let excess_sig_hex = excess_sig.get_signature().to_hex();

        debug!(
            target: LOG_TARGET,
            "Received transaction `{}` from peer `{}`",
            excess_sig_hex,
            self.peer_node_id.short_str()
        );
        let txn = Arc::new(txn);
        let store_state = self.mempool.has_transaction(txn.clone()).await?;
        if store_state.is_stored() {
            return Ok(());
        }

        let stored_result = self.mempool.insert(txn).await?;
        if stored_result.is_stored() {
            #[cfg(feature = "metrics")]
            metrics::inbound_transactions().inc();
            debug!(
                target: LOG_TARGET,
                "Inserted transaction `{}` from peer `{}`",
                excess_sig_hex,
                self.peer_node_id.short_str()
            );
        } else {
            #[cfg(feature = "metrics")]
            metrics::rejected_inbound_transactions().inc();
            debug!(
                target: LOG_TARGET,
                "Did not store new transaction `{excess_sig_hex}` in mempool: {stored_result}"
            )
        }

        Ok(())
    }

    async fn write_transactions(&mut self, transactions: Vec<Arc<Transaction>>) -> Result<(), MempoolProtocolError> {
        let txns = transactions.into_iter().take(self.config.initial_sync_max_transactions)
            .filter_map(|txn| {
                match shared_proto::types::Transaction::try_from(txn) {
                    Ok(txn) =>   Some(proto::TransactionItem {
                        transaction: Some(txn),
                    }),
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Could not convert transaction: {e}");
                        None
                    }
                }
            })
            // Write an empty `TransactionItem` to indicate we're done
            .chain(iter::once(proto::TransactionItem::empty()));

        self.write_messages(stream::iter(txns)).await?;

        Ok(())
    }

    async fn read_message<T: prost::Message + Default>(&mut self) -> Result<T, MempoolProtocolError> {
        let msg = time::timeout(MESSAGE_TIMEOUT, self.framed.next())
            .await
            .map_err(|_| MempoolProtocolError::RecvTimeout)?
            .ok_or_else(|| MempoolProtocolError::SubstreamClosed(self.peer_node_id.clone()))??;

        T::decode(&mut msg.freeze()).map_err(|err| MempoolProtocolError::DecodeFailed {
            source: err,
            peer: self.peer_node_id.clone(),
        })
    }

    async fn write_messages<S, T>(&mut self, stream: S) -> Result<(), MempoolProtocolError>
    where
        S: Stream<Item = T> + Unpin,
        T: prost::Message,
    {
        // `send_all` has no deadline, so a peer that stops reading stalls this task indefinitely.
        // Feeding item by item keeps the batching (one flush at the end) while bounding each step.
        let mut stream = stream.map(|m| Bytes::from(m.to_encoded_bytes()));
        while let Some(bytes) = stream.next().await {
            time::timeout(STREAM_ITEM_TIMEOUT, self.framed.feed(bytes))
                .await
                .map_err(|_| MempoolProtocolError::SendTimeout)??;
        }
        time::timeout(STREAM_ITEM_TIMEOUT, self.framed.flush())
            .await
            .map_err(|_| MempoolProtocolError::SendTimeout)??;
        Ok(())
    }

    async fn write_message<T: prost::Message>(&mut self, message: T) -> Result<(), MempoolProtocolError> {
        time::timeout(MESSAGE_TIMEOUT, self.framed.send(message.to_encoded_bytes().into()))
            .await
            .map_err(|_| MempoolProtocolError::SendTimeout)??;
        Ok(())
    }
}
