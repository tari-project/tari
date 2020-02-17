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
    chain_storage::BlockchainBackend,
    mempool::{
        mempool::Mempool,
        proto,
        service::{
            inbound_handlers::MempoolInboundHandlers,
            outbound_interface::OutboundMempoolServiceInterface,
            service::{MempoolService, MempoolStreams},
        },
        MempoolServiceConfig,
    },
    transactions::{proto::types::Transaction as ProtoTransaction, transaction::Transaction},
};
use futures::{channel::mpsc::unbounded as futures_mpsc_channel_unbounded, future, Future, Stream, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc};
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
use tokio::runtime;

const LOG_TARGET: &str = "base_node::mempool_service::initializer";

/// Initializer for the Mempool service and service future.
pub struct MempoolServiceInitializer<T>
where T: BlockchainBackend
{
    inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
    mempool: Mempool<T>,
    config: MempoolServiceConfig,
}

impl<T> MempoolServiceInitializer<T>
where T: BlockchainBackend
{
    /// Create a new MempoolServiceInitializer from the inbound message subscriber.
    pub fn new(
        inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
        mempool: Mempool<T>,
        config: MempoolServiceConfig,
    ) -> Self
    {
        Self {
            inbound_message_subscription_factory,
            mempool,
            config,
        }
    }

    /// Get a stream for inbound Mempool service request messages
    fn inbound_request_stream(&self) -> impl Stream<Item = DomainMessage<proto::MempoolServiceRequest>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::MempoolRequest)
            .map(map_decode::<proto::MempoolServiceRequest>)
            .filter_map(ok_or_skip_result)
    }

    /// Get a stream for inbound Mempool service response messages
    fn inbound_response_stream(&self) -> impl Stream<Item = DomainMessage<proto::MempoolServiceResponse>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::MempoolResponse)
            .map(map_decode::<proto::MempoolServiceResponse>)
            .filter_map(ok_or_skip_result)
    }

    /// Create a stream of 'New Transaction` messages
    fn inbound_transaction_stream(&self) -> impl Stream<Item = DomainMessage<Transaction>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::NewTransaction)
            .filter_map(extract_transaction)
    }
}

async fn extract_transaction(msg: Arc<PeerMessage>) -> Option<DomainMessage<Transaction>> {
    match msg.decode_message::<ProtoTransaction>() {
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Could not decode inbound transaction message. {}",
                e.to_string()
            );
            None
        },
        Ok(tx) => {
            let tx = match Transaction::try_from(tx) {
                Err(e) => {
                    let origin = msg.origin_public_key();
                    warn!(
                        target: LOG_TARGET,
                        "Inbound transaction message from {} was ill-formed. {}", origin, e
                    );
                    return None;
                },
                Ok(b) => b,
            };
            Some(DomainMessage {
                source_peer: msg.source_peer.clone(),
                dht_header: msg.dht_header.clone(),
                inner: tx,
            })
        },
    }
}

impl<T> ServiceInitializer for MempoolServiceInitializer<T>
where T: BlockchainBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        // Create streams for receiving Mempool service requests and response messages from comms
        let inbound_request_stream = self.inbound_request_stream();
        let inbound_response_stream = self.inbound_response_stream();
        let inbound_transaction_stream = self.inbound_transaction_stream();
        // Connect MempoolOutboundServiceHandle to MempoolService
        let (outbound_tx_sender_service, outbound_tx_stream) = futures_mpsc_channel_unbounded();
        let (outbound_request_sender_service, outbound_request_stream) = reply_channel::unbounded();
        let outbound_mp_interface =
            OutboundMempoolServiceInterface::new(outbound_request_sender_service, outbound_tx_sender_service);
        let executer_clone = executor.clone(); // Give MempoolService access to the executor
        let config = self.config;
        let mempool = self.mempool.clone();
        let inbound_handlers = MempoolInboundHandlers::new(mempool, outbound_mp_interface.clone());
        // Register handle to OutboundMempoolServiceInterface before waiting for handles to be ready
        handles_fut.register(outbound_mp_interface);

        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_message_service = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OutboundMessageRequester handle required for MempoolService");

            let streams = MempoolStreams::new(
                outbound_request_stream,
                outbound_tx_stream,
                inbound_request_stream,
                inbound_response_stream,
                inbound_transaction_stream,
            );
            let service =
                MempoolService::new(executer_clone, outbound_message_service, inbound_handlers, config).start(streams);
            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Mempool Service shutdown");
        });

        future::ready(Ok(()))
    }
}
