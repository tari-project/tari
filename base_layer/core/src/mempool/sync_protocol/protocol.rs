// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{HashMap, HashSet},
    iter,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{stream, SinkExt, Stream, StreamExt};
use libp2p_substream::Substream;
use log::{debug, warn};
use prost::Message;
use tari_network::{gossipsub::MessageId, identity::PeerId};
use tari_p2p::{
    framing::CanonicalFraming,
    proto as shared_proto,
    proto::{mempool as proto, mempool::MempoolSyncRequest},
};
use tari_rpc_framework::__macro_reexports::Bytes;
use tari_utilities::{hex::Hex, ByteArray};
use tokio::time;

#[cfg(feature = "metrics")]
use crate::mempool::metrics;
use crate::{
    mempool::{
        sync_protocol::{error::MempoolProtocolError, NewTransactionNotification},
        transaction_id::MempoolTransactionId,
        Mempool,
        MempoolError,
        MempoolServiceConfig,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::mempool::sync_protocol";
pub(super) struct MempoolPeerProtocol<'a> {
    config: &'a MempoolServiceConfig,
    framed: CanonicalFraming<Substream>,
    mempool: &'a Mempool,
    peer_id: PeerId,
}

impl<'a> MempoolPeerProtocol<'a> {
    pub fn new(
        config: &'a MempoolServiceConfig,
        framed: CanonicalFraming<Substream>,
        peer_id: PeerId,
        mempool: &'a Mempool,
    ) -> Self {
        Self {
            config,
            framed,
            mempool,
            peer_id,
        }
    }

    pub async fn request_transactions(
        &mut self,
        notifs: Vec<NewTransactionNotification>,
    ) -> RequestedTransactionProgress {
        let timer = Instant::now();
        debug!(target: LOG_TARGET, "Request transactions protocol started. Want {} transaction(s)", notifs.len());
        let progress = self.request_transactions_inner(notifs).await;
        debug!(target: LOG_TARGET, "Request transactions protocol complete in {:.2?}", timer.elapsed());
        if let Err(err) = self.framed.close().await {
            debug!(target: LOG_TARGET, "IO error when closing stream: {}", err);
        }
        progress
    }

    #[allow(clippy::too_many_lines)]
    pub async fn request_transactions_inner(
        &mut self,
        notifs: Vec<NewTransactionNotification>,
    ) -> RequestedTransactionProgress {
        let mut set = notifs
            .into_iter()
            .map(|n| (n.transaction_id, n.message_id))
            .collect::<HashMap<_, _>>();

        if set.is_empty() {
            debug!(target: LOG_TARGET, "No transactions to request");
            return RequestedTransactionProgress::default();
        }

        let num_requested = set.len();

        let ids = set.keys().map(|id| id.as_bytes().to_vec()).collect::<Vec<_>>();
        let request = MempoolSyncRequest::from(proto::RequestSpecificTransactions { ids });
        if let Err(err) = self.write_message(request).await {
            warn!(target: LOG_TARGET, "Failed to send request to peer {}: {}. All gossips will be ignored", self.peer_id, err);
            return RequestedTransactionProgress::ignore_many(set.into_values());
        }

        let mut accept = Vec::new();
        let mut reject = Vec::new();
        let mut ignore = Vec::new();

        let mut num_recv = 0;
        while let Some(result) = self.framed.next().await {
            let read_and_decode_result = result.map_err(MempoolProtocolError::IoError).and_then(|bytes| {
                proto::TransactionItem::decode(&mut bytes.freeze()).map_err(|err| MempoolProtocolError::DecodeFailed {
                    source: err,
                    peer: self.peer_id,
                })
            });
            let item = match read_and_decode_result {
                Ok(item) => item,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Error reading from stream: {err}. All gossip messages for peer {} will be ignored", self.peer_id);
                    return RequestedTransactionProgress::ignore_many(set.into_values());
                },
            };

            match item.transaction {
                Some(txn) => {
                    let Some(tx_id) = extract_transaction_id(&txn) else {
                        warn!(target: LOG_TARGET, "Peer returned an invalid transaction with no first kernel");
                        continue;
                    };
                    let Some(msg_id) = set.remove(&tx_id) else {
                        // Not requested
                        warn!(target: LOG_TARGET, "Peer sent transaction {tx_id} that was not requested.");
                        continue;
                    };
                    num_recv += 1;
                    debug!(target: LOG_TARGET, "Requested transaction {tx_id} received");
                    match self.validate_and_insert_transaction(txn).await {
                        Ok(stored) if stored.is_stored() => {
                            accept.push(msg_id);
                        },
                        Ok(stored) => {
                            warn!(target: LOG_TARGET, "Transaction {tx_id} was not stored: {stored}");
                            ignore.push(msg_id);
                        },
                        Err(MempoolProtocolError::MempoolError(MempoolError::TransactionError(err))) => {
                            warn!(target: LOG_TARGET, "Transaction {tx_id} failed validation: {err}");
                            reject.push(msg_id);
                        },
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Mempool protocol error: {err}");
                            ignore.push(msg_id);
                        },
                    }
                },
                None => {
                    debug!(
                        target: LOG_TARGET,
                        "All transaction(s) (count={}) received from peer `{}`. ",
                        num_recv,
                        self.peer_id,
                    );
                    break;
                },
            }

            if num_recv > num_requested {
                warn!(
                    target: LOG_TARGET,
                    "Peer sent more than the requested amount of transaction (num_recv={}) `{}`. ",
                    num_recv,
                    self.peer_id,
                );
                break;
            }
        }

        if !set.is_empty() {
            warn!(target: LOG_TARGET, "Not all transactions returned ({} received, {} remaining, {} requested). Ignoring all remaining", num_recv, set.len(), num_requested);
            ignore.extend(set.into_values());
        }

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_wrap)]
        #[cfg(feature = "metrics")]
        {
            match self.mempool.stats().await {
                Ok(stats) => {
                    metrics::unconfirmed_pool_size().set(stats.unconfirmed_txs as i64);
                    metrics::reorg_pool_size().set(stats.reorg_txs as i64);
                },
                Err(err) => {
                    warn!(target: LOG_TARGET, "mempool.stats() call failed when collecting metrics: {err}");
                },
            }
        }

        RequestedTransactionProgress { accept, ignore, reject }
    }

    pub async fn start_initiator_sync(mut self) -> Result<(), MempoolProtocolError> {
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
            self.peer_id,
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
            self.peer_id,
        );

        self.write_message(MempoolSyncRequest::from(inventory)).await?;

        self.read_and_insert_transactions_until_complete().await?;

        let missing_items: proto::InventoryIndexes = self.read_message().await?;
        debug!(
            target: LOG_TARGET,
            "Received {} missing transaction index(es) from peer `{}`",
            missing_items.indexes.len(),
            self.peer_id,
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
            self.peer_id,
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
            self.peer_id,
        );

        let proto::MempoolSyncRequest { request: Some(request) } = self.read_message().await? else {
            return Err(MempoolProtocolError::InvalidRequest {
                details: format!("Peer {} sent empty request", self.peer_id),
            });
        };

        match request {
            proto::mempool_sync_request::Request::Inventory(inv) => self.respond_to_inventory_request(inv).await,
            proto::mempool_sync_request::Request::Specific(req) => self.respond_to_specific_request(req).await,
        }
    }

    async fn respond_to_specific_request(
        &mut self,
        request: proto::RequestSpecificTransactions,
    ) -> Result<(), MempoolProtocolError> {
        if request.ids.len() > self.config.max_request_transactions {
            return Err(MempoolProtocolError::InvalidRequest {
                details: format!(
                    "Peer {} requested {} transactions (max: {})",
                    self.peer_id,
                    request.ids.len(),
                    self.config.max_request_transactions
                ),
            });
        }

        let requested_index = request.ids.iter().map(|s| s.as_bytes()).collect::<HashSet<_>>();

        let snapshot = self.mempool.snapshot().await?;
        for transaction in snapshot {
            let Some(excess_sig) = transaction.first_kernel_excess_sig() else {
                continue;
            };

            if requested_index.contains(excess_sig.get_signature().as_bytes()) {
                match shared_proto::common::Transaction::try_from(&*transaction) {
                    Ok(txn) => {
                        self.write_message(proto::TransactionItem { transaction: Some(txn) })
                            .await?;
                    },
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Could not convert transaction: {}", e);
                    },
                }
            }
        }

        // Empty message to indicate we are done
        self.write_message(proto::TransactionItem { transaction: None }).await?;
        if let Err(err) = self.framed.flush().await {
            debug!(target: LOG_TARGET, "IO error when flushing stream: {}", err);
        }

        debug!(target: LOG_TARGET, "Done responding to specific transaction request from peer {}", self.peer_id);
        Ok(())
    }

    async fn respond_to_inventory_request(
        &mut self,
        inventory: proto::TransactionInventory,
    ) -> Result<(), MempoolProtocolError> {
        debug!(
            target: LOG_TARGET,
            "Received inventory from peer `{}` containing {} item(s)",
            self.peer_id,
            inventory.items.len()
        );

        let transactions = self.mempool.snapshot().await?;
        let inventory_index = inventory
            .items
            .iter()
            .enumerate()
            .take(self.config.initial_sync_max_transactions)
            .map(|(idx, s)| (s.as_bytes(), idx))
            .collect::<HashMap<_, _>>();

        let mut duplicate_inventory_items = Vec::new();
        let (transactions, _) = transactions.into_iter().partition::<Vec<_>, _>(|transaction| {
            let Some(excess_sig) = transaction.first_kernel_excess_sig() else {
                return false;
            };

            match inventory_index.get(excess_sig.get_signature().as_bytes()) {
                Some(pos) => {
                    duplicate_inventory_items.push(*pos);
                    false
                },
                None => true,
            }
        });

        debug!(
            target: LOG_TARGET,
            "Streaming {} transaction(s) to peer `{}`",
            transactions.len(),
            self.peer_id,
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
            self.peer_id,
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
                    peer: self.peer_id,
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
                        self.peer_id,
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
        txn: shared_proto::common::Transaction,
    ) -> Result<TxStorageResponse, MempoolProtocolError> {
        let txn = Transaction::try_from(txn).map_err(|err| MempoolProtocolError::MessageConversionFailed {
            peer: self.peer_id,
            message: err,
        })?;
        let excess_sig = txn
            .first_kernel_excess_sig()
            .ok_or_else(|| MempoolProtocolError::ExcessSignatureMissing(self.peer_id))?;
        let excess_sig_hex = excess_sig.get_signature().to_hex();

        debug!(
            target: LOG_TARGET,
            "validate_and_insert_transaction: Received transaction `{}` from peer `{}`",
            excess_sig_hex,
            self.peer_id,
        );
        let txn = Arc::new(txn);
        let stored_result = self.mempool.has_transaction(txn.clone()).await?;
        if stored_result.is_stored() {
            return Ok(stored_result);
        }

        let stored_result = self.mempool.insert(txn).await?;
        if stored_result.is_stored() {
            #[cfg(feature = "metrics")]
            metrics::inbound_transactions().inc();
            debug!(
                target: LOG_TARGET,
                "Inserted transaction `{}` from peer `{}`",
                excess_sig_hex,
                self.peer_id,
            );
        } else {
            #[cfg(feature = "metrics")]
            metrics::rejected_inbound_transactions().inc();
            debug!(
                target: LOG_TARGET,
                "Did not store new transaction `{}` in mempool: {}", excess_sig_hex, stored_result
            );
        }

        Ok(stored_result)
    }

    async fn write_transactions(&mut self, transactions: Vec<Arc<Transaction>>) -> Result<(), MempoolProtocolError> {
        let txns = transactions.into_iter().take(self.config.initial_sync_max_transactions)
            .filter_map(|txn| {
                match shared_proto::common::Transaction::try_from(&*txn) {
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
            .ok_or_else(|| MempoolProtocolError::SubstreamClosed(self.peer_id))??;

        T::decode(&mut msg.freeze()).map_err(|err| MempoolProtocolError::DecodeFailed {
            source: err,
            peer: self.peer_id,
        })
    }

    async fn write_messages<S, T>(&mut self, stream: S) -> Result<(), MempoolProtocolError>
    where
        S: Stream<Item = T> + Unpin,
        T: prost::Message,
    {
        let mut s = stream.map(|m| Bytes::from(m.encode_to_vec())).map(Ok);
        self.framed.send_all(&mut s).await?;
        Ok(())
    }

    async fn write_message<T: prost::Message>(&mut self, message: T) -> Result<(), MempoolProtocolError> {
        time::timeout(
            Duration::from_secs(10),
            self.framed.send(message.encode_to_vec().into()),
        )
        .await
        .map_err(|_| MempoolProtocolError::SendTimeout)??;
        Ok(())
    }
}

fn extract_transaction_id(msg: &shared_proto::common::Transaction) -> Option<MempoolTransactionId> {
    msg.body
        .as_ref()
        .and_then(|b| b.kernels.first())
        .and_then(|k| k.excess_sig.as_ref())
        .and_then(|s| MempoolTransactionId::try_from(s.signature.as_slice()).ok())
}

#[derive(Default)]
pub(super) struct RequestedTransactionProgress {
    pub accept: Vec<MessageId>,
    pub reject: Vec<MessageId>,
    pub ignore: Vec<MessageId>,
}

impl RequestedTransactionProgress {
    pub fn ignore_many<I: IntoIterator<Item = MessageId>>(ignore: I) -> Self {
        Self {
            ignore: ignore.into_iter().collect(),
            ..Default::default()
        }
    }
}
