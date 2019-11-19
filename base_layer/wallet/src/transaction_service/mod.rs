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
pub mod error;
pub mod handle;
pub mod service;
pub mod storage;

use crate::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        handle::TransactionServiceHandle,
        service::TransactionService,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};
use futures::{future, Future, Stream, StreamExt};
use log::*;
use std::sync::Arc;
use tari_broadcast_channel::bounded;
use tari_comms::peer_manager::NodeIdentity;
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
use tari_transactions::{transaction_protocol::proto, types::CryptoFactories};
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "base_layer::wallet::transaction_service";

pub struct TransactionServiceInitializer<T>
where T: TransactionBackend
{
    subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
    backend: Option<T>,
    node_identity: Arc<NodeIdentity>,
    factories: CryptoFactories,
}

impl<T> TransactionServiceInitializer<T>
where T: TransactionBackend
{
    pub fn new(
        subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
        backend: T,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
    ) -> Self
    {
        Self {
            subscription_factory,
            backend: Some(backend),
            node_identity,
            factories,
        }
    }

    /// Get a stream of inbound Text messages
    fn transaction_stream(&self) -> impl Stream<Item = DomainMessage<proto::TransactionSenderMessage>> {
        self.subscription_factory
            .get_subscription(TariMessageType::Transaction)
            .map(map_decode::<proto::TransactionSenderMessage>)
            .filter_map(ok_or_skip_result)
    }

    fn transaction_reply_stream(&self) -> impl Stream<Item = DomainMessage<proto::RecipientSignedMessage>> {
        self.subscription_factory
            .get_subscription(TariMessageType::TransactionReply)
            .map(map_decode::<proto::RecipientSignedMessage>)
            .filter_map(ok_or_skip_result)
    }
}

impl<T> ServiceInitializer for TransactionServiceInitializer<T>
where T: TransactionBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: TaskExecutor,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let (sender, receiver) = reply_channel::unbounded();
        let transaction_stream = self.transaction_stream();
        let transaction_reply_stream = self.transaction_reply_stream();

        let (publisher, subscriber) = bounded(100);

        let transaction_handle = TransactionServiceHandle::new(sender, subscriber);

        // Register handle before waiting for handles to be ready
        handles_fut.register(transaction_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Transaction Service without providing a backend");

        let node_identity = self.node_identity.clone();
        let factories = self.factories.clone();
        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_message_service = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OMS handle required for TransactionService");
            let output_manager_service = handles
                .get_handle::<OutputManagerHandle>()
                .expect("Output Manager Service handle required for TransactionService");

            let service = TransactionService::new(
                TransactionDatabase::new(backend),
                receiver,
                transaction_stream,
                transaction_reply_stream,
                output_manager_service,
                outbound_message_service,
                publisher,
                node_identity,
                factories,
            )
            .start();
            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Transaction Service shutdown");
        });

        future::ready(Ok(()))
    }
}
