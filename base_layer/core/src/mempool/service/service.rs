// Copyright 2019 The Tari Project
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

use std::{convert::TryFrom, sync::Arc};

use futures::{Stream, pin_mut, stream::StreamExt};
use log::*;
use tari_comms::peer_manager::NodeId;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester},
};
use tari_p2p::{comms_connector::PeerMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::RequestContext};
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::hex::Hex;
use tokio::{sync::mpsc, task};

use crate::{
    base_node::comms_interface::{BlockEvent, BlockEventReceiver},
    common::inbound_backpressure::InboundBackpressure,
    mempool::service::{
        MempoolRequest,
        MempoolResponse,
        error::MempoolServiceError,
        inbound_handlers::MempoolInboundHandlers,
    },
    proto,
};

const LOG_TARGET: &str = "c::mempool::service::service";

/// The maximum number of inbound `NewTransaction` messages accepted but not yet handled (the historical
/// `BoundedExecutor` size). Further messages are dropped until one finishes.
const MAX_PENDING_INBOUND_TRANSACTIONS: usize = 100;

/// A convenience struct to hold all the Mempool service streams
pub struct MempoolStreams<STxIn, SLocalReq> {
    pub outbound_tx_stream: mpsc::UnboundedReceiver<(Arc<Transaction>, Vec<NodeId>)>,
    /// Raw `NewTransaction` messages; decoding happens off the service loop
    pub inbound_transaction_stream: STxIn,
    pub local_request_stream: SLocalReq,
    pub block_event_stream: BlockEventReceiver,
    pub request_receiver: reply_channel::TryReceiver<MempoolRequest, MempoolResponse, MempoolServiceError>,
}

/// The Mempool Service is responsible for handling inbound requests and responses and for sending new requests to the
/// Mempools of remote Base nodes.
pub struct MempoolService {
    outbound_message_service: OutboundMessageRequester,
    inbound_handlers: MempoolInboundHandlers,
    pending_inbound_transactions: InboundBackpressure,
}

impl MempoolService {
    pub fn new(outbound_message_service: OutboundMessageRequester, inbound_handlers: MempoolInboundHandlers) -> Self {
        Self {
            outbound_message_service,
            inbound_handlers,
            pending_inbound_transactions: InboundBackpressure::new(
                MAX_PENDING_INBOUND_TRANSACTIONS,
                "transaction",
                LOG_TARGET,
            ),
        }
    }

    pub async fn start<STxIn, SLocalReq>(
        mut self,
        streams: MempoolStreams<STxIn, SLocalReq>,
    ) -> Result<(), MempoolServiceError>
    where
        STxIn: Stream<Item = Arc<PeerMessage>>,
        SLocalReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
    {
        let mut outbound_tx_stream = streams.outbound_tx_stream;
        let inbound_transaction_stream = streams.inbound_transaction_stream.fuse();
        pin_mut!(inbound_transaction_stream);
        let local_request_stream = streams.local_request_stream.fuse();
        pin_mut!(local_request_stream);
        let mut block_event_stream = streams.block_event_stream;
        let mut request_receiver = streams.request_receiver;

        loop {
            tokio::select! {
                // Requests sent from the handle
                Some(request) = request_receiver.next() => {
                    let (request, reply) = request.split();
                    let _result = reply.send(self.handle_request(request).await);
                },

                // Outbound tx messages from the OutboundMempoolServiceInterface
                Some((txn, excluded_peers)) = outbound_tx_stream.recv() => {
                    let outbound_message_service = self.outbound_message_service.clone();
                    task::spawn(async move {
                        if let Err(e) = Self::handle_outbound_tx(outbound_message_service, txn, excluded_peers).await {
                            error!(target: LOG_TARGET, "Error sending outbound tx message: {e}");
                        }
                    });
                },

                // Incoming transaction messages from the Comms layer
                Some(transaction_msg) = inbound_transaction_stream.next() => {
                    let _spawned = self.handle_incoming_tx(transaction_msg);
                },

                // Incoming local request messages from the LocalMempoolServiceInterface and other local services
                Some(local_request_context) = local_request_stream.next() => {
                    self.spawn_handle_local_request(local_request_context);
                },

                // Block events from local Base Node.
                block_event = block_event_stream.recv() => {
                    if let Ok(block_event) = block_event {
                        self.spawn_handle_block_event(block_event);
                    }
                },


                else => {
                    info!(target: LOG_TARGET, "Mempool service shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&mut self, request: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        self.inbound_handlers.handle_request(request).await
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    ) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let result = reply_tx.send(inbound_handlers.handle_request(request).await);

            if let Err(res) = result {
                error!(
                    target: LOG_TARGET,
                    "MempoolService failed to send reply to local request {:?}",
                    res.map(|r| r.to_string()).map_err(|e| e.to_string())
                );
            }
        });
    }

    fn spawn_handle_block_event(&self, block_event: Arc<BlockEvent>) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let result = inbound_handlers.handle_block_event(&block_event).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle base node block event: {e}");
            }
        });
    }

    /// Handle a raw inbound transaction message. Decoding and validation are both done in a spawned task, under a
    /// single mempool validation permit, so that the service loop is never blocked decoding a transaction.
    ///
    /// At most [MAX_PENDING_INBOUND_TRANSACTIONS] messages are handled (or waiting for a validation permit) at once;
    /// beyond that, the message is dropped. Returns `true` if a task was spawned to handle the message.
    fn handle_incoming_tx(&mut self, msg: Arc<PeerMessage>) -> bool {
        let Some(pending_permit) = self.pending_inbound_transactions.try_accept() else {
            #[cfg(feature = "metrics")]
            crate::mempool::metrics::rejected_inbound_transactions().inc();
            return false;
        };
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            // Released when the task finishes, however it finishes
            let _pending_permit = pending_permit;
            let result = inbound_handlers.handle_transaction_message(msg).await;
            if let Err(e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming transaction message: {e:?}"
                );
            }
        });
        true
    }

    async fn handle_outbound_tx(
        mut outbound_message_service: OutboundMessageRequester,
        tx: Arc<Transaction>,
        exclude_peers: Vec<NodeId>,
    ) -> Result<(), MempoolServiceError> {
        let result = outbound_message_service
            .flood(
                NodeDestination::Unknown,
                OutboundEncryption::ClearText,
                exclude_peers,
                OutboundDomainMessage::new(
                    &TariMessageType::NewTransaction,
                    proto::types::Transaction::try_from(tx.clone()).map_err(MempoolServiceError::ConversionError)?,
                ),
                format!(
                    "Outbound mempool tx: {}",
                    tx.first_kernel_excess_sig()
                        .map(|s| s.get_signature().to_hex())
                        .unwrap_or_else(|| "No kernels!".to_string())
                ),
            )
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(DhtOutboundError::NoMessagesQueued) => Ok(()),
            Err(e) => {
                error!(target: LOG_TARGET, "Handle outbound tx failure. {e:?}");
                Err(MempoolServiceError::OutboundMessageService(e.to_string()))
            },
        }
    }
}

#[cfg(test)]
mod test {
    use prost::Message;
    use tari_transaction_components::{MicroMinotari, key_manager::KeyManager, tx};

    use super::*;
    use crate::{
        mempool::{Mempool, MempoolConfig, OutboundMempoolServiceInterface},
        test_helpers::{create_consensus_rules, create_peer_message},
        validation::mocks::MockValidator,
    };

    #[tokio::test]
    async fn inbound_transactions_are_dropped_when_too_many_are_pending() {
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        let mempool = Mempool::new(config, create_consensus_rules(), Box::new(MockValidator::new(true)));
        let (tx_sender, _tx_receiver) = mpsc::unbounded_channel();
        let inbound_handlers =
            MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
        let (outbound_sender, _outbound_receiver) = mpsc::unbounded_channel();
        let mut service = MempoolService::new(OutboundMessageRequester::new(outbound_sender), inbound_handlers);

        let key_manager = KeyManager::new_random().unwrap();
        let tx = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let body = proto::types::Transaction::try_from(tx.clone()).unwrap().encode_to_vec();
        let msg = create_peer_message(TariMessageType::NewTransaction, body);

        // Every pending slot is taken: the message is dropped without spawning a task
        let held = service
            .pending_inbound_transactions
            .semaphore()
            .acquire_many_owned(u32::try_from(MAX_PENDING_INBOUND_TRANSACTIONS).unwrap())
            .await
            .unwrap();
        assert!(!service.handle_incoming_tx(msg.clone()));
        tokio::task::yield_now().await;
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
        drop(held);
        assert_eq!(
            service.pending_inbound_transactions.available(),
            MAX_PENDING_INBOUND_TRANSACTIONS
        );

        // With room, it is handled, and the slot is released once the task finishes
        assert!(service.handle_incoming_tx(msg));
        // A malformed message also releases its slot
        assert!(service.handle_incoming_tx(create_peer_message(TariMessageType::NewTransaction, vec![0xff; 16])));
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while service.pending_inbound_transactions.available() != MAX_PENDING_INBOUND_TRANSACTIONS {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("pending slots were not released");
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 1);
    }
}
