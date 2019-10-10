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
pub mod model;
mod service;

use self::service::TextMessageService;
use crate::text_message_service::{handle::TextMessageHandle, model::ReceivedTextMessage, service::TextMessageAck};
use futures::{future, stream::StreamExt, Future, Stream};
use log::*;
use std::sync::Arc;
use tari_broadcast_channel::bounded;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_p2p::{
    comms_connector::PeerMessage,
    domain_message::DomainMessage,
    services::{
        liveness::handle::LivenessHandle,
        utils::{map_deserialized, ok_or_skip_result},
    },
    tari_message::{ExtendedMessage, TariMessageType},
};
use tari_pubsub::TopicSubscriptionFactory;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "wallet::text_message_service::initializer";

pub struct TextMessageServiceInitializer {
    pub_key: Option<CommsPublicKey>,
    database_path: Option<String>,
    subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>>,
}

impl TextMessageServiceInitializer {
    pub fn new(
        subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>>,
        pub_key: CommsPublicKey,
        database_path: String,
    ) -> Self
    {
        Self {
            pub_key: Some(pub_key),
            database_path: Some(database_path),
            subscription_factory,
        }
    }

    /// Get a stream of inbound Text messages
    fn text_message_stream(&self) -> impl Stream<Item = DomainMessage<ReceivedTextMessage>> {
        self.subscription_factory
            .get_subscription(TariMessageType::new(ExtendedMessage::Text))
            .map(map_deserialized::<ReceivedTextMessage>)
            .filter_map(ok_or_skip_result)
    }

    fn text_message_ack_stream(&self) -> impl Stream<Item = DomainMessage<TextMessageAck>> {
        self.subscription_factory
            .get_subscription(TariMessageType::new(ExtendedMessage::TextAck))
            .map(map_deserialized::<TextMessageAck>)
            .filter_map(ok_or_skip_result)
    }
}

impl ServiceInitializer for TextMessageServiceInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, executor: TaskExecutor, handles_fut: ServiceHandlesFuture) -> Self::Future {
        let pub_key = self
            .pub_key
            .take()
            .expect("text message service initializer already called");

        let database_path = self
            .database_path
            .take()
            .expect("text message service initializer already called");

        let (sender, receiver) = reply_channel::unbounded();

        let (publisher, subscriber) = bounded(100);

        let text_message_stream = self.text_message_stream();
        let text_message_ack_stream = self.text_message_ack_stream();

        let tms_handle = TextMessageHandle::new(sender, subscriber);

        // Register handle before waiting for handles to be ready
        handles_fut.register(tms_handle);

        executor.spawn(async move {
            let handles = handles_fut.await;

            let oms = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("OMS handle required for TextMessageService");
            let liveness = handles
                .get_handle::<LivenessHandle>()
                .expect("Liveness handle required for TextMessageService");

            let service = TextMessageService::new(
                receiver,
                text_message_stream,
                text_message_ack_stream,
                pub_key,
                database_path,
                oms,
                liveness,
                publisher,
            );

            match service.start().await {
                Ok(_) => {
                    info!(target: LOG_TARGET, "Text message service initializer exited cleanly");
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "Text message service failed to start: {}", err);
                },
            }
        });

        future::ready(Ok(()))
    }
}
