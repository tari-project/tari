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

use std::{sync::Arc, time::Duration};

use futures::{Stream, StreamExt, future};
use log::*;
use tari_comms::connectivity::ConnectivityRequester;
use tari_comms_dht::Dht;
use tari_node_components::blocks::NewBlock;
use tari_p2p::{
    comms_connector::{PeerMessage, SubscriptionFactory},
    domain_message::DomainMessage,
    services::utils::map_decode_with_max_items,
    tari_message::TariMessageType,
};
use tari_service_framework::{
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
    async_trait,
    reply_channel,
};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};

use crate::{
    base_node::{
        BaseNodeStateMachineConfig,
        StateMachineHandle,
        comms_interface::{InboundNodeCommsHandlers, LocalNodeCommsInterface, OutboundNodeCommsInterface},
        service::service::{BaseNodeService, BaseNodeStreams},
    },
    chain_storage::{BlockchainBackend, async_db::AsyncBlockchainDb},
    consensus::BaseNodeConsensusManager,
    mempool::Mempool,
    proof_of_work::randomx_factory::RandomXFactory,
    proto as shared_protos,
    proto::base_node as proto,
};

const LOG_TARGET: &str = "c::bn::service::initializer";
const SUBSCRIPTION_LABEL: &str = "Base Node";

/// Initializer for the Base Node service handle and service future.
pub struct BaseNodeServiceInitializer<T> {
    inbound_message_subscription_factory: Arc<SubscriptionFactory>,
    blockchain_db: AsyncBlockchainDb<T>,
    mempool: Mempool,
    consensus_manager: BaseNodeConsensusManager,
    service_request_timeout: Duration,
    randomx_factory: RandomXFactory,
    base_node_config: BaseNodeStateMachineConfig,
}

impl<T> BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    /// Create a new BaseNodeServiceInitializer from the inbound message subscriber.
    pub fn new(
        inbound_message_subscription_factory: Arc<SubscriptionFactory>,
        blockchain_db: AsyncBlockchainDb<T>,
        mempool: Mempool,
        consensus_manager: BaseNodeConsensusManager,
        service_request_timeout: Duration,
        randomx_factory: RandomXFactory,
        base_node_config: BaseNodeStateMachineConfig,
    ) -> Self {
        Self {
            inbound_message_subscription_factory,
            blockchain_db,
            mempool,
            consensus_manager,
            service_request_timeout,
            randomx_factory,
            base_node_config,
        }
    }

    /// Get a stream for inbound Base Node request messages
    fn inbound_request_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>> + use<T> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::BaseNodeRequest, SUBSCRIPTION_LABEL)
            // The only list a request carries is `ExcessSigs`: the kernel excess signatures of a `NewBlock` that the
            // requester is missing from its mempool. A `NewBlock` is decoded under this same budget, so a legitimate
            // request never needs more, and without it one 8 MiB frame of empty signatures decodes into ~4M `Vec`s.
            .map(map_decode_with_max_items::<proto::BaseNodeServiceRequest>(
                shared_protos::MESSAGE_MAX_DECODE_ITEMS,
            ))
    }

    /// Get a stream for inbound Base Node response messages
    fn inbound_response_stream(
        &self,
    ) -> impl Stream<Item = DomainMessage<Result<proto::BaseNodeServiceResponse, prost::DecodeError>>> + use<T> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::BaseNodeResponse, SUBSCRIPTION_LABEL)
            .map(map_decode_with_max_items::<proto::BaseNodeServiceResponse>(
                shared_protos::MESSAGE_MAX_DECODE_ITEMS,
            ))
    }

    /// Create a stream of raw 'New Block` messages. The messages are decoded off the service loop, on a blocking
    /// thread, under a block decode permit (see `BaseNodeService::spawn_handle_incoming_block`).
    fn inbound_block_stream(&self) -> impl Stream<Item = Arc<PeerMessage>> + use<T> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::NewBlock, SUBSCRIPTION_LABEL)
    }
}

#[derive(Error, Debug)]
pub enum ExtractBlockError {
    #[error("Could not decode inbound block message. {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Inbound block message was ill-formed. {0}")]
    MalformedMessage(String),
}

/// Decode an inbound `NewBlock` message. This is CPU-bound: it protobuf-decodes the whole message (up to the messaging
/// frame size), converts the header and the coinbase outputs and kernels, and parses every kernel excess signature
/// scalar canonically. It must be run on a blocking thread.
pub(crate) fn extract_block(msg: &PeerMessage) -> DomainMessage<Result<NewBlock, ExtractBlockError>> {
    let new_block = match msg
        .decode_message_with_max_items::<shared_protos::core::NewBlock>(shared_protos::MESSAGE_MAX_DECODE_ITEMS)
    {
        Ok(block) => block,
        Err(e) => {
            return DomainMessage {
                source_peer: msg.source_peer.clone(),
                dht_header: msg.dht_header.clone(),
                authenticated_origin: msg.authenticated_origin.clone(),
                inner: Err(e.into()),
            };
        },
    };
    let block = NewBlock::try_from(new_block).map_err(ExtractBlockError::MalformedMessage);
    DomainMessage {
        source_peer: msg.source_peer.clone(),
        dht_header: msg.dht_header.clone(),
        authenticated_origin: msg.authenticated_origin.clone(),
        inner: block,
    }
}

#[async_trait]
impl<T> ServiceInitializer for BaseNodeServiceInitializer<T>
where T: BlockchainBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        trace!(target: LOG_TARGET, "Initializing Base Node Service");
        // Create streams for receiving Base Node requests and response messages from comms
        let inbound_request_stream = self.inbound_request_stream();
        let inbound_response_stream = self.inbound_response_stream();
        let inbound_block_stream = self.inbound_block_stream();
        // Connect InboundNodeCommsInterface and OutboundNodeCommsInterface to BaseNodeService
        let (outbound_request_sender_service, outbound_request_stream) = reply_channel::unbounded();
        let (outbound_block_sender_service, outbound_block_stream) = mpsc::unbounded_channel();
        let (local_request_sender_service, local_request_stream) = reply_channel::unbounded();
        let (local_block_sender_service, local_block_stream) = reply_channel::unbounded();
        let outbound_nci =
            OutboundNodeCommsInterface::new(outbound_request_sender_service, outbound_block_sender_service);
        let (block_event_sender, _) = broadcast::channel(50);
        let local_nci = LocalNodeCommsInterface::new(
            local_request_sender_service,
            local_block_sender_service,
            block_event_sender.clone(),
        );

        // Register handle to OutboundNodeCommsInterface before waiting for handles to be ready
        context.register_handle(outbound_nci.clone());
        context.register_handle(local_nci);

        let service_request_timeout = self.service_request_timeout;
        let blockchain_db = self.blockchain_db.clone();
        let mempool = self.mempool.clone();
        let consensus_manager = self.consensus_manager.clone();
        let randomx_factory = self.randomx_factory.clone();
        let config = self.base_node_config.clone();

        context.spawn_when_ready(move |handles| async move {
            let dht = handles.expect_handle::<Dht>();
            let connectivity = handles.expect_handle::<ConnectivityRequester>();
            let outbound_message_service = dht.outbound_requester();

            let state_machine = handles.expect_handle::<StateMachineHandle>();

            let inbound_nch = InboundNodeCommsHandlers::new(
                block_event_sender,
                blockchain_db,
                mempool,
                consensus_manager,
                outbound_nci.clone(),
                connectivity.clone(),
                randomx_factory,
            );

            let streams = BaseNodeStreams {
                outbound_request_stream,
                outbound_block_stream,
                inbound_request_stream,
                inbound_response_stream,
                inbound_block_stream,
                local_request_stream,
                local_block_stream,
            };
            let service = BaseNodeService::new(
                outbound_message_service,
                inbound_nch,
                service_request_timeout,
                state_machine,
                connectivity,
                config,
            )
            .start(streams);
            futures::pin_mut!(service);
            future::select(service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Base Node Service shutdown");
        });

        debug!(target: LOG_TARGET, "Base Node Service initialized");
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use prost::Message;
    use tari_p2p::tari_message::TariMessageType;

    use super::*;
    use crate::test_helpers::create_peer_message;

    /// `key, length` header of field `tag`
    fn len_header(tag: u32, len: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        prost::encoding::encode_key(tag, prost::encoding::WireType::LengthDelimited, &mut buf);
        prost::encoding::encode_varint(len as u64, &mut buf);
        buf
    }

    #[test]
    fn an_over_budget_new_block_message_is_a_decode_error() {
        // A frame of empty `NewBlock.coinbase_outputs` (tag 3), built from wire bytes
        let body = len_header(3, 0).repeat(4_000_000);
        let msg = create_peer_message(TariMessageType::NewBlock, body);
        let decoded = extract_block(&msg);
        assert!(
            matches!(&decoded.inner, Err(ExtractBlockError::DecodeError(err)) if err.to_string().contains("decode budget"))
        );
    }

    #[test]
    fn an_over_budget_base_node_response_is_a_decode_error() {
        // `BaseNodeServiceResponse.fetch_mempool_transactions_by_excess_sigs_response` (tag 7) holding a frame of empty
        // `transactions` (tag 1)
        let transactions = len_header(1, 0).repeat(4_000_000);
        let mut body = len_header(7, transactions.len());
        body.extend_from_slice(&transactions);
        let msg = create_peer_message(TariMessageType::BaseNodeResponse, body);
        let decoded =
            map_decode_with_max_items::<proto::BaseNodeServiceResponse>(shared_protos::MESSAGE_MAX_DECODE_ITEMS)(msg);
        assert!(matches!(&decoded.inner, Err(err) if err.to_string().contains("decode budget")));
    }

    #[test]
    fn an_over_budget_base_node_request_is_a_decode_error() {
        // `BaseNodeServiceRequest.fetch_mempool_transactions_by_excess_sigs` (tag 9) holding a frame of empty
        // `excess_sigs` (tag 1)
        let request_of_empty_sigs = |count: usize| {
            let excess_sigs = len_header(1, 0).repeat(count);
            let mut body = len_header(9, excess_sigs.len());
            body.extend_from_slice(&excess_sigs);
            create_peer_message(TariMessageType::BaseNodeRequest, body)
        };
        // Prost alone accepts a flood just over the budget. Only that one is decoded: prost-decoding the full-frame
        // flood below would cost ~135 MB, and the lib tests run in parallel.
        let just_over = request_of_empty_sigs(shared_protos::MESSAGE_MAX_DECODE_ITEMS + 1);
        assert!(<proto::BaseNodeServiceRequest as prost::Message>::decode(just_over.body.as_slice()).is_ok());
        let decoded = map_decode_with_max_items::<proto::BaseNodeServiceRequest>(
            shared_protos::MESSAGE_MAX_DECODE_ITEMS,
        )(just_over);
        assert!(matches!(&decoded.inner, Err(err) if err.to_string().contains("decode budget")));

        // A full 8 MiB frame of empty signatures is rejected without being decoded
        let msg = request_of_empty_sigs(4_000_000);
        let decoded =
            map_decode_with_max_items::<proto::BaseNodeServiceRequest>(shared_protos::MESSAGE_MAX_DECODE_ITEMS)(msg);
        assert!(matches!(&decoded.inner, Err(err) if err.to_string().contains("decode budget")));

        // A request for every kernel excess signature of a max-size `NewBlock` passes
        let request = proto::BaseNodeServiceRequest {
            request_key: 1,
            request: Some(
                proto::base_node_service_request::Request::FetchMempoolTransactionsByExcessSigs(proto::ExcessSigs {
                    excess_sigs: vec![vec![1u8; 32]; 8_994],
                }),
            ),
        };
        let msg = create_peer_message(
            TariMessageType::BaseNodeRequest,
            prost::Message::encode_to_vec(&request),
        );
        let decoded =
            map_decode_with_max_items::<proto::BaseNodeServiceRequest>(shared_protos::MESSAGE_MAX_DECODE_ITEMS)(msg);
        assert_eq!(decoded.inner.unwrap(), request);
    }

    #[test]
    fn malformed_new_block_messages_are_errors() {
        // Not a protobuf message at all
        let msg = create_peer_message(TariMessageType::NewBlock, vec![0xff; 64]);
        let decoded = extract_block(&msg);
        assert_eq!(decoded.source_peer.node_id, msg.source_peer.node_id);
        assert!(matches!(decoded.inner, Err(ExtractBlockError::DecodeError(_))));

        // A valid protobuf message that is not a valid block
        let body = shared_protos::core::NewBlock::default().encode_to_vec();
        let msg = create_peer_message(TariMessageType::NewBlock, body);
        let decoded = extract_block(&msg);
        assert!(matches!(decoded.inner, Err(ExtractBlockError::MalformedMessage(_))));
    }
}
