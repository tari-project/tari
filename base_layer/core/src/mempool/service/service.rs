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
use tokio::{
    sync::{Semaphore, mpsc, oneshot},
    task,
};

use crate::{
    base_node::comms_interface::{BlockEvent, BlockEventReceiver},
    common::inbound_backpressure::PendingByPeer,
    mempool::{
        MempoolError,
        service::{
            MempoolRequest,
            MempoolResponse,
            error::MempoolServiceError,
            inbound_handlers::MempoolInboundHandlers,
        },
    },
    proto,
};

const LOG_TARGET: &str = "c::mempool::service::service";

/// The maximum number of inbound `NewTransaction` messages from a single peer that are accepted but not yet handled.
/// Further messages from that peer are dropped until one finishes; other peers are unaffected.
const MAX_PENDING_INBOUND_TRANSACTIONS_PER_PEER: usize = 32;
/// A backstop on the number of inbound `NewTransaction` messages pending from all peers together, only reached if many
/// peers flood at once.
const MAX_PENDING_INBOUND_TRANSACTIONS_TOTAL: usize = 512;

/// The maximum number of `SubmitTransaction` requests from the mempool handle (which serves remote peers through the
/// P2P mempool RPC service) handled at once. Requests are handled in their own tasks, so without this they could take
/// every validation permit ahead of gossiped transactions. Read-only requests are not limited.
const MAX_CONCURRENT_HANDLE_SUBMISSIONS: usize = 2;
/// The same bound for `SubmitTransaction` requests from the local service (gRPC and JSON-RPC). A separate pool, so that
/// remote peers cannot starve local submissions.
const MAX_CONCURRENT_LOCAL_SUBMISSIONS: usize = MAX_CONCURRENT_HANDLE_SUBMISSIONS;

type MempoolReply = oneshot::Sender<Result<MempoolResponse, MempoolServiceError>>;

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
    pending_inbound_transactions: PendingByPeer,
    handle_submission_permits: Arc<Semaphore>,
    local_submission_permits: Arc<Semaphore>,
}

impl MempoolService {
    pub fn new(outbound_message_service: OutboundMessageRequester, inbound_handlers: MempoolInboundHandlers) -> Self {
        Self {
            outbound_message_service,
            inbound_handlers,
            pending_inbound_transactions: PendingByPeer::new(
                MAX_PENDING_INBOUND_TRANSACTIONS_PER_PEER,
                MAX_PENDING_INBOUND_TRANSACTIONS_TOTAL,
                "transaction",
                LOG_TARGET,
            ),
            handle_submission_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_HANDLE_SUBMISSIONS)),
            local_submission_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_LOCAL_SUBMISSIONS)),
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
                // Handled in a task, so that a submission waiting for a validation permit never stalls this loop
                Some(request) = request_receiver.next() => {
                    self.spawn_handle_request(request);
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

    fn spawn_handle_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    ) {
        let inbound_handlers = self.inbound_handlers.clone();
        let submission_permits = self.handle_submission_permits.clone();
        task::spawn(async move {
            let (request, reply) = request_context.split();
            // The requester may have stopped waiting; nothing to do then
            let _result = handle_request(inbound_handlers, request, reply, submission_permits).await;
        });
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    ) {
        let inbound_handlers = self.inbound_handlers.clone();
        let submission_permits = self.local_submission_permits.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let result = handle_request(inbound_handlers, request, reply_tx, submission_permits).await;

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
    /// At most [MAX_PENDING_INBOUND_TRANSACTIONS_PER_PEER] messages per peer (and
    /// [MAX_PENDING_INBOUND_TRANSACTIONS_TOTAL] overall) are handled, or waiting for a validation permit, at once;
    /// beyond that, the message is dropped. Returns `true` if a task was spawned to handle the message.
    fn handle_incoming_tx(&mut self, msg: Arc<PeerMessage>) -> bool {
        let Some(pending_guard) = self.pending_inbound_transactions.try_accept(&msg.source_peer.node_id) else {
            #[cfg(feature = "metrics")]
            crate::mempool::metrics::rejected_inbound_transactions().inc();
            return false;
        };
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            // Released when the task finishes, however it finishes
            let _pending_guard = pending_guard;
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

/// Handles one request from the mempool handle or the local service, and replies to it. A `SubmitTransaction` first
/// takes one of `submission_permits`. A request whose requester has stopped waiting (e.g. an RPC call that timed out
/// at the client's deadline) is dropped without being handled: this is checked while it waits for a permit, and
/// raced against its handling, so that an abandoned submission never takes a validation permit.
async fn handle_request(
    mut inbound_handlers: MempoolInboundHandlers,
    request: MempoolRequest,
    mut reply: MempoolReply,
    submission_permits: Arc<Semaphore>,
) -> Result<(), Result<MempoolResponse, MempoolServiceError>> {
    let _permit = if matches!(request, MempoolRequest::SubmitTransaction(_)) {
        tokio::select! {
            biased;
            () = reply.closed() => {
                debug!(target: LOG_TARGET, "Dropping an abandoned transaction submission");
                return Ok(());
            },
            permit = submission_permits.acquire_owned() => match permit {
                Ok(permit) => Some(permit),
                Err(e) => {
                    return reply.send(Err(MempoolServiceError::MempoolError(MempoolError::InternalError(format!(
                        "Submission semaphore closed: {e}"
                    )))));
                },
            },
        }
    } else {
        None
    };
    let result = tokio::select! {
        biased;
        () = reply.closed() => {
            debug!(target: LOG_TARGET, "Dropping an abandoned mempool request");
            return Ok(());
        },
        result = inbound_handlers.handle_request(request) => result,
    };
    reply.send(result)
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
    async fn inbound_transactions_are_dropped_when_a_peer_has_too_many_pending() {
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
        let msg = create_peer_message(TariMessageType::NewTransaction, body.clone());
        let flooder = msg.source_peer.node_id.clone();

        // The flooding peer has as many messages pending as it may
        let held = (0..MAX_PENDING_INBOUND_TRANSACTIONS_PER_PEER)
            .map(|_| service.pending_inbound_transactions.try_accept(&flooder).unwrap())
            .collect::<Vec<_>>();
        // Its next message is dropped without spawning a task
        assert!(!service.handle_incoming_tx(msg.clone()));
        tokio::task::yield_now().await;
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);

        // Another peer's message is still accepted and handled, and so is a malformed one from a third peer
        // (each test message comes from a new random peer)
        let other = create_peer_message(TariMessageType::NewTransaction, body.clone());
        assert_ne!(other.source_peer.node_id, flooder);
        assert!(service.handle_incoming_tx(other));
        assert!(service.handle_incoming_tx(create_peer_message(TariMessageType::NewTransaction, vec![0xff; 16])));
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while service.pending_inbound_transactions.pending_total() != MAX_PENDING_INBOUND_TRANSACTIONS_PER_PEER {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("pending slots were not released");
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 1);

        // Every count returns to zero once the flooder's tasks finish
        drop(held);
        assert_eq!(service.pending_inbound_transactions.pending_for(&flooder), 0);
        assert_eq!(service.pending_inbound_transactions.pending_total(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_blocked_submission_does_not_stall_other_requests() {
        use futures::stream;

        use crate::mempool::{TxStorageResponse, service::MempoolHandle};

        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        let mempool = Mempool::new(config, create_consensus_rules(), Box::new(MockValidator::new(true)));
        let (tx_sender, _tx_receiver) = mpsc::unbounded_channel();
        let inbound_handlers =
            MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
        let (outbound_sender, _outbound_receiver) = mpsc::unbounded_channel();
        let service = MempoolService::new(OutboundMessageRequester::new(outbound_sender), inbound_handlers);

        let (_outbound_tx_sender, outbound_tx_stream) = mpsc::unbounded_channel();
        let (block_event_sender, block_event_stream) = tokio::sync::broadcast::channel(1);
        let (request_sender, request_receiver) = reply_channel::unbounded();
        let streams = MempoolStreams {
            outbound_tx_stream,
            inbound_transaction_stream: stream::pending::<Arc<PeerMessage>>(),
            local_request_stream: stream::pending::<
                RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
            >(),
            block_event_stream,
            request_receiver,
        };
        let service_task = tokio::spawn(service.start(streams));
        let mut handle = MempoolHandle::new(request_sender);

        // Every validation permit is taken, so a submission waits for one
        let mut held = Vec::new();
        while let Ok(Ok(permit)) = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            mempool.acquire_validation_permit(),
        )
        .await
        {
            held.push(permit);
        }
        let key_manager = KeyManager::new_random().unwrap();
        let tx = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
            .expect("Failed to get tx")
            .0;
        let submit = tokio::spawn({
            let mut handle = handle.clone();
            async move { handle.submit_transaction(tx).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!submit.is_finished());

        // An unrelated request is still served
        let stats = tokio::time::timeout(std::time::Duration::from_secs(10), handle.get_stats())
            .await
            .expect("the service loop was stalled by the pending submission")
            .unwrap();
        assert_eq!(stats.unconfirmed_txs, 0);

        // Once a permit is free, the submission completes
        drop(held);
        let response = tokio::time::timeout(std::time::Duration::from_secs(10), submit)
            .await
            .expect("submission did not complete")
            .unwrap()
            .unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        drop(block_event_sender);
        service_task.abort();
    }

    /// Counts validations, records the maximum number of concurrent ones, and takes a while
    #[derive(Default)]
    struct CountingValidator {
        calls: std::sync::atomic::AtomicUsize,
        current: std::sync::atomic::AtomicUsize,
        max: std::sync::atomic::AtomicUsize,
    }

    impl crate::validation::TransactionValidator for Arc<CountingValidator> {
        fn validate_full(&self, tx: &Transaction) -> Result<(), crate::validation::ValidationError> {
            self.validate_chain_linked(tx)
        }

        fn validate_chain_linked(&self, _tx: &Transaction) -> Result<(), crate::validation::ValidationError> {
            use std::sync::atomic::Ordering;
            self.calls.fetch_add(1, Ordering::SeqCst);
            let current = self.current.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            self.max.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(200));
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        fn validate_internal_consistency(
            &self,
            _tx: &Transaction,
            _tip: Option<&tari_common_types::chain_metadata::ChainMetadata>,
        ) -> Result<(), crate::validation::ValidationError> {
            Ok(())
        }
    }

    struct RunningService {
        mempool: Mempool,
        validator: Arc<CountingValidator>,
        handle: crate::mempool::service::MempoolHandle,
        local: crate::mempool::service::LocalMempoolService,
        handle_submission_permits: Arc<Semaphore>,
        task: task::JoinHandle<Result<(), MempoolServiceError>>,
        _block_events: tokio::sync::broadcast::Sender<Arc<BlockEvent>>,
        _outbound: mpsc::UnboundedSender<(Arc<Transaction>, Vec<NodeId>)>,
        _propagated: mpsc::UnboundedReceiver<(Arc<Transaction>, Vec<NodeId>)>,
    }

    fn start_service() -> RunningService {
        use futures::stream;

        let validator = Arc::new(CountingValidator::default());
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        let mempool = Mempool::new(config, create_consensus_rules(), Box::new(validator.clone()));
        let (tx_sender, propagated) = mpsc::unbounded_channel();
        let inbound_handlers =
            MempoolInboundHandlers::new(mempool.clone(), OutboundMempoolServiceInterface::new(tx_sender));
        let (outbound_sender, _outbound_receiver) = mpsc::unbounded_channel();
        let service = MempoolService::new(OutboundMessageRequester::new(outbound_sender), inbound_handlers);
        let handle_submission_permits = service.handle_submission_permits.clone();

        let (outbound_tx_sender, outbound_tx_stream) = mpsc::unbounded_channel();
        let (block_event_sender, block_event_stream) = tokio::sync::broadcast::channel(1);
        let (request_sender, request_receiver) = reply_channel::unbounded();
        let (local_sender, local_request_stream) = reply_channel::unbounded();
        let streams = MempoolStreams {
            outbound_tx_stream,
            inbound_transaction_stream: stream::pending::<Arc<PeerMessage>>(),
            local_request_stream,
            block_event_stream,
            request_receiver,
        };
        RunningService {
            mempool,
            validator,
            handle: crate::mempool::service::MempoolHandle::new(request_sender),
            local: crate::mempool::service::LocalMempoolService::new(local_sender),
            handle_submission_permits,
            task: tokio::spawn(service.start(streams)),
            _block_events: block_event_sender,
            _outbound: outbound_tx_sender,
            _propagated: propagated,
        }
    }

    fn new_tx(key_manager: &KeyManager) -> Transaction {
        tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, key_manager)
            .expect("Failed to get tx")
            .0
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn handle_submissions_are_bounded() {
        use std::sync::atomic::Ordering;
        let service = start_service();
        let key_manager = KeyManager::new_random().unwrap();
        let tasks = (0..3)
            .map(|_| {
                let tx = new_tx(&key_manager);
                let mut handle = service.handle.clone();
                tokio::spawn(async move { handle.submit_transaction(tx).await })
            })
            .collect::<Vec<_>>();
        for task in tasks {
            assert_eq!(
                task.await.unwrap().unwrap(),
                crate::mempool::TxStorageResponse::UnconfirmedPool
            );
        }
        let max = service.validator.max.load(Ordering::SeqCst);
        assert!(
            (1..=MAX_CONCURRENT_HANDLE_SUBMISSIONS).contains(&max),
            "{max} concurrent submissions"
        );
        service.task.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_abandoned_submission_is_not_validated() {
        use std::sync::atomic::Ordering;
        let service = start_service();
        let key_manager = KeyManager::new_random().unwrap();
        // Every handle submission slot is taken
        let held = service
            .handle_submission_permits
            .clone()
            .acquire_many_owned(u32::try_from(MAX_CONCURRENT_HANDLE_SUBMISSIONS).unwrap())
            .await
            .unwrap();
        // A submission queues for a slot, and its requester then gives up (e.g. an RPC deadline)
        let submit = tokio::spawn({
            let mut handle = service.handle.clone();
            let tx = new_tx(&key_manager);
            async move { handle.submit_transaction(tx).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(!submit.is_finished());
        submit.abort();
        let _cancelled = submit.await;

        // Once a slot is free, the abandoned submission is dropped instead of being validated
        drop(held);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(service.validator.calls.load(Ordering::SeqCst), 0);
        assert_eq!(service.mempool.stats().await.unwrap().unconfirmed_txs, 0);
        assert_eq!(
            service.handle_submission_permits.available_permits(),
            MAX_CONCURRENT_HANDLE_SUBMISSIONS
        );
        service.task.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_local_submission_is_served_while_the_handle_pool_is_saturated() {
        let mut service = start_service();
        let key_manager = KeyManager::new_random().unwrap();
        let _held = service
            .handle_submission_permits
            .clone()
            .acquire_many_owned(u32::try_from(MAX_CONCURRENT_HANDLE_SUBMISSIONS).unwrap())
            .await
            .unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            service.local.submit_transaction(new_tx(&key_manager)),
        )
        .await
        .expect("the local submission waited for the handle pool")
        .unwrap();
        assert_eq!(response, crate::mempool::TxStorageResponse::UnconfirmedPool);
        service.task.abort();
    }
}
