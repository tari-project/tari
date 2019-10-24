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
        comms_interface::{InboundNodeCommsInterface, OutboundNodeCommsInterface},
        service::{
            service::{BaseNodeService, BaseNodeServiceConfig},
            service_request::BaseNodeServiceRequest,
            service_response::BaseNodeServiceResponse,
        },
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use futures::{future, Future, Stream, StreamExt};
use log::*;
use std::sync::Arc;
use tari_comms::peer_manager::NodeIdentity;
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_p2p::{
    comms_connector::PeerMessage,
    domain_message::DomainMessage,
    services::utils::{map_deserialized, ok_or_skip_result},
    tari_message::{BlockchainMessage, TariMessageType},
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

const LOG_TARGET: &'static str = "tari_core::base_node::base_node_service";

/// Initializer for the Base Node service handle and service future.
pub struct BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    inbound_message_subscription_factory:
        Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>>,
    node_identity: Arc<NodeIdentity>,
    blockchain_db: BlockchainDatabase<T>,
    config: BaseNodeServiceConfig,
}

impl<T> BaseNodeServiceInitializer<T>
where T: BlockchainBackend
{
    /// Create a new BaseNodeServiceInitializer from the inbound message subscriber.
    pub fn new(
        inbound_message_subscription_factory: Arc<
            TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>,
        >,
        node_identity: Arc<NodeIdentity>,
        blockchain_db: BlockchainDatabase<T>,
        config: BaseNodeServiceConfig,
    ) -> Self
    {
        Self {
            inbound_message_subscription_factory,
            node_identity,
            blockchain_db,
            config,
        }
    }

    /// Get a stream for inbound Base Node request messages
    fn inbound_request_stream(&self) -> impl Stream<Item = DomainMessage<BaseNodeServiceRequest>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::new(BlockchainMessage::BaseNodeRequest))
            .map(map_deserialized::<BaseNodeServiceRequest>)
            .filter_map(ok_or_skip_result)
    }

    /// Get a stream for inbound Base Node response messages
    fn inbound_response_stream(&self) -> impl Stream<Item = DomainMessage<BaseNodeServiceResponse>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::new(BlockchainMessage::BaseNodeResponse))
            .map(map_deserialized::<BaseNodeServiceResponse>)
            .filter_map(ok_or_skip_result)
    }

    // TODO: add streams for broadcasted blocks and transactions
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
        let node_identity = self.node_identity.clone();
        // Connect InboundNodeCommsInterface and OutboundNodeCommsInterface to BaseNodeService
        let (outbound_request_sender_service, outbound_request_stream) = reply_channel::unbounded();
        let outbound_nci = OutboundNodeCommsInterface::new(outbound_request_sender_service);
        let inbound_nci = Arc::new(InboundNodeCommsInterface::new(self.blockchain_db.clone()));
        let executer_clone = executor.clone(); // Give BaseNodeService access to the executor
        let config = self.config.clone();

        // Register handle to OutboundNodeCommsInterface before waiting for handles to be ready
        handles_fut.register(outbound_nci);

        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_message_service = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OutboundMessageRequester handle required for BaseNodeService");

            let service = BaseNodeService::new(
                executer_clone,
                outbound_request_stream,
                inbound_request_stream,
                inbound_response_stream,
                outbound_message_service,
                node_identity,
                inbound_nci,
                config,
            )
            .start();
            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Base Node Service shutdown");
        });

        future::ready(Ok(()))
    }
}
