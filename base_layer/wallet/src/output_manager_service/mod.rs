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
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::database::{OutputManagerBackend, OutputManagerDatabase},
    },
    transaction_service::handle::TransactionServiceHandle,
};
use futures::{future, Future, Stream, StreamExt};
use log::*;
use std::sync::Arc;
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_core::{
    base_node::proto::base_node as BaseNodeProto,
    consensus::{ConsensusConstantsBuilder, Network},
    transactions::types::CryptoFactories,
};
use tari_p2p::{
    comms_connector::SubscriptionFactory,
    domain_message::DomainMessage,
    services::utils::{map_decode, ok_or_skip_result},
    tari_message::TariMessageType,
};
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};
use tari_shutdown::ShutdownSignal;
use tokio::{runtime, sync::broadcast};

pub mod config;
pub mod error;
pub mod handle;
pub mod protocols;
#[allow(unused_assignments)]
pub mod service;
pub mod storage;

const LOG_TARGET: &str = "wallet::output_manager_service::initializer";
const SUBSCRIPTION_LABEL: &str = "Output Manager";

pub type TxId = u64;

pub struct OutputManagerServiceInitializer<T>
where T: OutputManagerBackend
{
    config: OutputManagerServiceConfig,
    subscription_factory: Arc<SubscriptionFactory>,
    backend: Option<T>,
    factories: CryptoFactories,
    network: Network,
}

impl<T> OutputManagerServiceInitializer<T>
where T: OutputManagerBackend + Clone + 'static
{
    pub fn new(
        config: OutputManagerServiceConfig,
        subscription_factory: Arc<SubscriptionFactory>,
        backend: T,
        factories: CryptoFactories,
        network: Network,
    ) -> Self
    {
        Self {
            config,
            subscription_factory,
            backend: Some(backend),
            factories,
            network,
        }
    }

    fn base_node_response_stream(&self) -> impl Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>> {
        trace!(
            target: LOG_TARGET,
            "Subscription '{}' for topic '{:?}' created.",
            SUBSCRIPTION_LABEL,
            TariMessageType::BaseNodeResponse
        );
        self.subscription_factory
            .get_subscription(TariMessageType::BaseNodeResponse, SUBSCRIPTION_LABEL)
            .map(map_decode::<BaseNodeProto::BaseNodeServiceResponse>)
            .filter_map(ok_or_skip_result)
    }
}

impl<T> ServiceInitializer for OutputManagerServiceInitializer<T>
where T: OutputManagerBackend + Clone + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let base_node_response_stream = self.base_node_response_stream();

        let (sender, receiver) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(200);

        let oms_handle = OutputManagerHandle::new(sender, publisher.clone());

        // Register handle before waiting for handles to be ready
        handles_fut.register(oms_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Output Manager Service without setting a storage backend");
        let factories = self.factories.clone();
        let config = self.config.clone();
        let constants = ConsensusConstantsBuilder::new(self.network).build();

        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_message_service = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OMS handle required for Output Manager Service");

            let transaction_service = handles
                .get_handle::<TransactionServiceHandle>()
                .expect("Transaction Service handle required for Output Manager Service");

            let service = OutputManagerService::new(
                config,
                outbound_message_service,
                transaction_service,
                receiver,
                base_node_response_stream,
                OutputManagerDatabase::new(backend),
                publisher,
                factories,
                constants.coinbase_lock_height(),
            )
            .await
            .expect("Could not initialize Output Manager Service")
            .start();

            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Output manager service shutdown");
        });
        future::ready(Ok(()))
    }
}
