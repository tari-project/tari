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
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use error::MempoolProtocolError;
use futures::{stream, SinkExt, Stream, StreamExt};
pub use initializer::MempoolSyncInitializer;
use log::*;
use prost::Message;
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityEventRx},
    framing,
    framing::CanonicalFraming,
    message::MessageExt,
    peer_manager::{NodeId, PeerFeatures},
    protocol::{ProtocolEvent, ProtocolNotification, ProtocolNotificationRx},
    Bytes,
    PeerConnection,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_utilities::ByteArray;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Semaphore,
    task,
    time,
};

use crate::{
    mempool::{metrics, proto, Mempool, MempoolServiceConfig},
    proto as shared_proto,
    transactions::transaction_components::Transaction,
};

#[cfg(test)]
mod test;

mod error;
mod initializer;

const MAX_FRAME_SIZE: usize = 3 * 1024 * 1024; // 3 MiB
const LOG_TARGET: &str = "c::mempool::sync_protocol";

pub static MEMPOOL_SYNC_PROTOCOL: Bytes = Bytes::from_static(b"t/mempool-sync/1");

pub struct MempoolSyncProtocol<TSubstream> {
    config: MempoolServiceConfig,
    protocol_notifier: ProtocolNotificationRx<TSubstream>,
    connectivity_events: ConnectivityEventRx,
    mempool: Mempool,
    num_synched: Arc<AtomicUsize>,
    permits: Arc<Semaphore>,
}

impl<TSubstream> MempoolSyncProtocol<TSubstream>
where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static
{
    pub fn new(
        config: MempoolServiceConfig,
        protocol_notifier: ProtocolNotificationRx<TSubstream>,
        connectivity_events: ConnectivityEventRx,
        mempool: Mempool,
    ) -> Self {
        Self {
            config,
            protocol_notifier,
            connectivity_events,
            mempool,
            num_synched: Arc::new(AtomicUsize::new(0)),
            permits: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Mempool protocol handler has started");

        loop {
            tokio::select! {
                Ok(event) = self.connectivity_events.recv() => {
                    self.handle_connectivity_event(event).await;
                },

                Some(notif) = self.protocol_notifier.recv() => {
                    self.handle_protocol_notification(notif);
                }
            }
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
                    self.spawn_initiator_protocol(conn.clone()).await;
                }
            },
            _ => {},
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
        let config = self.config;
        task::spawn(async move {
            // Only initiate this protocol with a single peer at a time
            let _permit = permits.acquire().await;
            if num_synched.load(Ordering::SeqCst) >= config.initial_sync_num_peers {
                return;
            }
            match conn.open_framed_substream(&MEMPOOL_SYNC_PROTOCOL, MAX_FRAME_SIZE).await {
                Ok(framed) => {
                    let protocol = MempoolPeerProtocol::new(config, framed, conn.peer_node_id().clone(), mempool);
                    match protocol.start_initiator().await {
                        Ok(_) => {
                            debug!(
                                target: LOG_TARGET,
                                "Mempool initiator protocol completed successfully for peer `{}`",
                                conn.peer_node_id().short_str(),
                            );
                            num_synched.fetch_add(1, Ordering::SeqCst);
                        },
                        Err(err) => {
                            debug!(
                                target: LOG_TARGET,
                                "Mempool initiator protocol failed for peer `{}`: {}",
                                conn.peer_node_id().short_str(),
                                err
                            );
                        },
                    }
                },
                Err(err) => error!(
                    target: LOG_TARGET,
                    "Unable to establish mempool protocol substream to peer `{}`: {}",
                    conn.peer_node_id().short_str(),
                    err
                ),
            }
        });
    }

    fn spawn_inbound_handler(&self, node_id: NodeId, substream: TSubstream) {
        let mempool = self.mempool.clone();
        let config = self.config;
        task::spawn(async move {
            let framed = framing::canonical(substream, MAX_FRAME_SIZE);
            let mut protocol = MempoolPeerProtocol::new(config, framed, node_id.clone(), mempool);
            match protocol.start_responder().await {
                Ok(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool responder protocol succeeded for peer `{}`",
                        node_id.short_str()
                    );
                },
                Err(err) => {
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
                if let Err(err) = self.framed.flush().await {
                    debug!(target: LOG_TARGET, "IO error when flushing stream: {}", err);
                }
                if let Err(err) = self.framed.close().await {
                    debug!(target: LOG_TARGET, "IO error when closing stream: {}", err);
                }
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

        // Close the stream after writing
        self.framed.close().await?;

        Ok(())
    }

    pub async fn start_responder(&mut self) -> Result<(), MempoolProtocolError> {
        match self.start_responder_inner().await {
            Ok(_) => {
                debug!(target: LOG_TARGET, "Responder protocol complete");
                Ok(())
            },
            Err(err) => {
                if let Err(err) = self.framed.flush().await {
                    debug!(target: LOG_TARGET, "IO error when flushing stream: {}", err);
                }
                if let Err(err) = self.framed.close().await {
                    debug!(target: LOG_TARGET, "IO error when closing stream: {}", err);
                }
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
        let mut num_recv = 0;
        while let Some(result) = self.framed.next().await {
            let bytes = result?;
            let item = proto::TransactionItem::decode(&mut bytes.freeze()).map_err(|err| {
                MempoolProtocolError::DecodeFailed {
                    source: err,
                    peer: self.peer_node_id.clone(),
                }
            })?;

            match item.transaction {
                Some(txn) => {
                    self.validate_and_insert_transaction(txn).await?;
                    num_recv += 1;
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

        let stats = self.mempool.stats().await?;
        metrics::unconfirmed_pool_size().set(stats.unconfirmed_txs as i64);
        metrics::reorg_pool_size().set(stats.reorg_txs as i64);

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
            metrics::inbound_transactions(Some(&self.peer_node_id)).inc();
            debug!(
                target: LOG_TARGET,
                "Inserted transaction `{}` from peer `{}`",
                excess_sig_hex,
                self.peer_node_id.short_str()
            );
        } else {
            metrics::rejected_inbound_transactions(Some(&self.peer_node_id)).inc();
            debug!(
                target: LOG_TARGET,
                "Did not store new transaction `{}` in mempool: {}", excess_sig_hex, stored_result
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
                        warn!(target: LOG_TARGET, "Could not convert transaction: {}", e);
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
        let msg = time::timeout(Duration::from_secs(10), self.framed.next())
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
        let mut s = stream.map(|m| Bytes::from(m.to_encoded_bytes())).map(Ok);
        self.framed.send_all(&mut s).await?;
        Ok(())
    }

    async fn write_message<T: prost::Message>(&mut self, message: T) -> Result<(), MempoolProtocolError> {
        time::timeout(
            Duration::from_secs(10),
            self.framed.send(message.to_encoded_bytes().into()),
        )
        .await
        .map_err(|_| MempoolProtocolError::SendTimeout)??;
        Ok(())
    }
}
