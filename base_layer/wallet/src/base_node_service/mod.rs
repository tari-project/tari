// Copyright 2020. The Tari Project
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
pub mod service;

use crate::base_node_service::{
    config::BaseNodeServiceConfig,
    handle::BaseNodeServiceHandle,
    service::BaseNodeService,
};

use log::*;
use std::sync::Arc;
use tari_comms_dht::Dht;

use futures::{future, Future, Stream, StreamExt};
use tari_core::base_node::proto::base_node as BaseNodeProto;
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

const LOG_TARGET: &str = "wallet::base_node_service";
const SUBSCRIPTION_LABEL: &str = "Base Node";

pub struct BaseNodeServiceInitializer {
    config: BaseNodeServiceConfig,
    subscription_factory: Arc<SubscriptionFactory>,
}

impl BaseNodeServiceInitializer {
    pub fn new(config: BaseNodeServiceConfig, subscription_factory: Arc<SubscriptionFactory>) -> Self {
        Self {
            config,
            subscription_factory,
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
impl ServiceInitializer for BaseNodeServiceInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        info!(target: LOG_TARGET, "Wallet base node service initializing.");

        let (sender, request_stream) = reply_channel::unbounded();
        let base_node_response_stream = self.base_node_response_stream();

        let (event_publisher, _) = broadcast::channel(200);

        let basenode_service_handle = BaseNodeServiceHandle::new(sender, event_publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(basenode_service_handle);

        let config = self.config.clone();

        context.spawn_when_ready(move |handles| async move {
            let dht = handles.expect_handle::<Dht>();
            let outbound_messaging = dht.outbound_requester();

            let service = BaseNodeService::new(
                config,
                base_node_response_stream,
                request_stream,
                outbound_messaging,
                event_publisher,
                handles.get_shutdown_signal(),
            )
            .start();
            futures::pin_mut!(service);
            future::select(service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Wallet Base Node Service shutdown");
        });

        future::ready(Ok(()))
    }
}
