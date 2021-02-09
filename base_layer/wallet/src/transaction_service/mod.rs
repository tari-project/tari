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

pub mod config;
pub mod error;
pub mod handle;
pub mod protocols;
pub mod service;
pub mod storage;
pub mod tasks;

use crate::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        service::TransactionService,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};
use futures::{future, Future, Stream, StreamExt};
use log::*;
use std::sync::Arc;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeIdentity};
use tari_comms_dht::Dht;
use tari_core::{
    proto::base_node as base_node_proto,
    transactions::{transaction_protocol::proto, types::CryptoFactories},
};
use tari_p2p::{
    comms_connector::SubscriptionFactory,
    domain_message::DomainMessage,
    services::utils::{map_decode, ok_or_skip_result},
    tari_message::TariMessageType,
};
use tari_service_framework::{
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::broadcast;

const LOG_TARGET: &str = "wallet::transaction_service";
const SUBSCRIPTION_LABEL: &str = "Transaction Service";

pub struct TransactionServiceInitializer<T>
where T: TransactionBackend
{
    config: TransactionServiceConfig,
    subscription_factory: Arc<SubscriptionFactory>,
    backend: Option<T>,
    node_identity: Arc<NodeIdentity>,
    factories: CryptoFactories,
}

impl<T> TransactionServiceInitializer<T>
where T: TransactionBackend
{
    pub fn new(
        config: TransactionServiceConfig,
        subscription_factory: Arc<SubscriptionFactory>,
        backend: T,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
    ) -> Self
    {
        Self {
            config,
            subscription_factory,
            backend: Some(backend),
            node_identity,
            factories,
        }
    }

    /// Get a stream of inbound Text messages
    fn transaction_stream(&self) -> impl Stream<Item = DomainMessage<proto::TransactionSenderMessage>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::SenderPartialTransaction
        );
        self.subscription_factory
            .get_subscription(TariMessageType::SenderPartialTransaction, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionSenderMessage>)
            .filter_map(ok_or_skip_result)
    }

    fn transaction_reply_stream(&self) -> impl Stream<Item = DomainMessage<proto::RecipientSignedMessage>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::ReceiverPartialTransactionReply
        );
        self.subscription_factory
            .get_subscription(TariMessageType::ReceiverPartialTransactionReply, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::RecipientSignedMessage>)
            .filter_map(ok_or_skip_result)
    }

    fn transaction_finalized_stream(&self) -> impl Stream<Item = DomainMessage<proto::TransactionFinalizedMessage>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::TransactionFinalized
        );
        self.subscription_factory
            .get_subscription(TariMessageType::TransactionFinalized, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionFinalizedMessage>)
            .filter_map(ok_or_skip_result)
    }

    fn base_node_response_stream(&self) -> impl Stream<Item = DomainMessage<base_node_proto::BaseNodeServiceResponse>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::BaseNodeResponse
        );
        self.subscription_factory
            .get_subscription(TariMessageType::BaseNodeResponse, SUBSCRIPTION_LABEL)
            .map(map_decode::<base_node_proto::BaseNodeServiceResponse>)
            .filter_map(ok_or_skip_result)
    }

    fn transaction_cancelled_stream(&self) -> impl Stream<Item = DomainMessage<proto::TransactionCancelledMessage>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::TransactionCancelled
        );
        self.subscription_factory
            .get_subscription(TariMessageType::TransactionCancelled, SUBSCRIPTION_LABEL)
            .map(map_decode::<proto::TransactionCancelledMessage>)
            .filter_map(ok_or_skip_result)
    }
}

impl<T> ServiceInitializer for TransactionServiceInitializer<T>
where T: TransactionBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        let (sender, receiver) = reply_channel::unbounded();
        let transaction_stream = self.transaction_stream();
        let transaction_reply_stream = self.transaction_reply_stream();
        let transaction_finalized_stream = self.transaction_finalized_stream();
        let base_node_response_stream = self.base_node_response_stream();
        let transaction_cancelled_stream = self.transaction_cancelled_stream();

        let (publisher, _) = broadcast::channel(200);

        let transaction_handle = TransactionServiceHandle::new(sender, publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(transaction_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Transaction Service without providing a backend");

        let node_identity = self.node_identity.clone();
        let factories = self.factories.clone();
        let config = self.config.clone();

        context.spawn_when_ready(move |handles| async move {
            let outbound_message_service = handles.expect_handle::<Dht>().outbound_requester();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let connectivity_manager = handles.expect_handle::<ConnectivityRequester>();

            let service = TransactionService::new(
                config,
                TransactionDatabase::new(backend),
                receiver,
                transaction_stream,
                transaction_reply_stream,
                transaction_finalized_stream,
                base_node_response_stream,
                transaction_cancelled_stream,
                output_manager_service,
                outbound_message_service,
                connectivity_manager,
                publisher,
                node_identity,
                factories,
                handles.get_shutdown_signal(),
            )
            .start();
            futures::pin_mut!(service);
            future::select(service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Transaction Service shutdown");
        });

        future::ready(Ok(()))
    }
}
