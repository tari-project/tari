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

use crate::{
    base_node::{
        comms_interface::{
            InboundNodeCommsHandlers,
            InboundNodeCommsHandlersConfig,
            LocalNodeCommsInterface,
            OutboundNodeCommsInterface,
        },
        proto,
        service::service::{BaseNodeService, BaseNodeServiceConfig, BaseNodeStreams},
    },
    blocks::Block,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    mempool::Mempool,
    proto as shared_protos,
};
use futures::{channel::mpsc::unbounded as futures_mpsc_channel_unbounded, future, Future, Stream, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc};
use tari_broadcast_channel::bounded;
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_p2p::{
    comms_connector::PeerMessage,
    domain_message::DomainMessage,
    services::utils::{map_decode, ok_or_skip_result},
    tari_message::TariMessageType,
};
use tari_pubsub::TopicSubscriptionFactory;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};
use tari_shutdown::ShutdownSignal;
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "base_node::service::initializer";

/// Initializer for the Base Node service handle and service future.
pub struct BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
    blockchain_db: BlockchainDatabase<T>,
    mempool: Mempool<T>,
    consensus_manager: ConsensusManager<T>,
    base_node_service_config: BaseNodeServiceConfig,
    inbound_node_comms_handlers_config: InboundNodeCommsHandlersConfig,
}

impl<T> BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    /// Create a new BaseNodeServiceInitializer from the inbound message subscriber.
    pub fn new(
        inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
        blockchain_db: BlockchainDatabase<T>,
        mempool: Mempool<T>,
        consensus_manager: ConsensusManager<T>,
        base_node_service_config: BaseNodeServiceConfig,
        inbound_node_comms_handlers_config: InboundNodeCommsHandlersConfig,
    ) -> Self
    {
        Self {
            inbound_message_subscription_factory,
            blockchain_db,
            mempool,
            consensus_manager,
            base_node_service_config,
            inbound_node_comms_handlers_config,
        }
    }

    /// Get a stream for inbound Base Node request messages
    fn inbound_request_stream(&self) -> impl Stream<Item = DomainMessage<proto::BaseNodeServiceRequest>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::BaseNodeRequest)
            .map(map_decode::<proto::BaseNodeServiceRequest>)
            .filter_map(ok_or_skip_result)
    }

    /// Get a stream for inbound Base Node response messages
    fn inbound_response_stream(&self) -> impl Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::BaseNodeResponse)
            .map(map_decode::<proto::BaseNodeServiceResponse>)
            .filter_map(ok_or_skip_result)
    }

    /// Create a stream of 'New Block` messages
    fn inbound_block_stream(&self) -> impl Stream<Item = DomainMessage<Block>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::NewBlock)
            .filter_map(extract_block)
    }
}

async fn extract_block(msg: Arc<PeerMessage>) -> Option<DomainMessage<Block>> {
    match msg.decode_message::<shared_protos::core::Block>() {
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Could not decode inbound block message. {}",
                e.to_string()
            );
            None
        },
        Ok(block) => {
            let block = match Block::try_from(block) {
                Err(e) => {
                    let origin = &msg.dht_header.origin_public_key;
                    warn!(
                        target: LOG_TARGET,
                        "Inbound block message from {} was ill-formed. {}", origin, e
                    );
                    return None;
                },
                Ok(b) => b,
            };
            Some(DomainMessage {
                source_peer: msg.source_peer.clone(),
                dht_header: msg.dht_header.clone(),
                inner: block,
            })
        },
    }
}

impl<T> ServiceInitializer for BaseNodeServiceInitializer<T>
where T: BlockchainBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: TaskExecutor,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        // Create streams for receiving Base Node requests and response messages from comms
        let inbound_request_stream = self.inbound_request_stream();
        let inbound_response_stream = self.inbound_response_stream();
        let inbound_block_stream = self.inbound_block_stream();
        // Connect InboundNodeCommsInterface and OutboundNodeCommsInterface to BaseNodeService
        let (outbound_request_sender_service, outbound_request_stream) = reply_channel::unbounded();
        let (outbound_block_sender_service, outbound_block_stream) = futures_mpsc_channel_unbounded();
        let (local_request_sender_service, local_request_stream) = reply_channel::unbounded();
        let (local_block_sender_service, local_block_stream) = reply_channel::unbounded();
        let outbound_nci =
            OutboundNodeCommsInterface::new(outbound_request_sender_service, outbound_block_sender_service.clone());
        let (block_event_publisher, block_event_subscriber) = bounded(100);
        let local_nci = LocalNodeCommsInterface::new(
            local_request_sender_service,
            local_block_sender_service,
            block_event_subscriber,
        );
        let inbound_nch = InboundNodeCommsHandlers::new(
            block_event_publisher,
            self.blockchain_db.clone(),
            self.mempool.clone(),
            self.consensus_manager.clone(),
            outbound_nci.clone(),
            self.inbound_node_comms_handlers_config,
        );
        let executer_clone = executor.clone(); // Give BaseNodeService access to the executor
        let base_node_service_config = self.base_node_service_config.clone();

        // Register handle to OutboundNodeCommsInterface before waiting for handles to be ready
        handles_fut.register(outbound_nci);
        handles_fut.register(local_nci);

        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_message_service = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OutboundMessageRequester handle required for BaseNodeService");

            let streams = BaseNodeStreams::new(
                outbound_request_stream,
                outbound_block_stream,
                inbound_request_stream,
                inbound_response_stream,
                inbound_block_stream,
                local_request_stream,
                local_block_stream,
            );
            let service = BaseNodeService::new(
                executer_clone,
                outbound_message_service,
                inbound_nch,
                base_node_service_config,
            )
            .start(streams);
            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Base Node Service shutdown");
        });

        future::ready(Ok(()))
    }
}
