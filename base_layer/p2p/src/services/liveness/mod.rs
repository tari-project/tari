//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! # Liveness Service
//!
//! This service is responsible for sending pings to any peer as well as maintaining
//! some very basic counters for the number of ping/pongs sent and received.
//!
//! It consists of:
//! - A service handle which makes requests to the Liveness backend. Types of requests can be found in the
//!   [LivenessRequest] enum.
//! - A handler for incoming [PingPong] messages.
//!
//! In future, this service may be expanded to included periodic pings to maintain
//! latency and availability statistics for peers.
//!
//! [LivenessRequest]: ./messages/enum.LivenessRequets.html
//! [PingPong]: ./messages/enum.PingPong.html

mod error;
mod handler;
mod messages;
mod service;
mod state;

use self::{error::LivenessError, handler::LivenessHandler, service::LivenessService, state::LivenessState};
use crate::{
    services::{
        comms_outbound::CommsOutboundHandle,
        domain_deserializer::DomainMessageDeserializer,
        ServiceHandlesFuture,
        ServiceName,
    },
    tari_message::{NetMessage, TariMessageType},
};
use futures::{future, Future, Stream};
use log::*;
use std::{fmt::Debug, sync::Arc};
use tari_comms::{builder::CommsServices, domain_subscriber::MessageInfo};
use tari_service_framework::{
    transport::{self, Requester},
    ServiceInitializationError,
    ServiceInitializer,
};

pub use self::messages::{LivenessRequest, LivenessResponse, PingPong};
use tari_comms::inbound_message_service::InboundTopicSubscriptionFactory;

pub type LivenessHandle = Requester<LivenessRequest, Result<LivenessResponse, LivenessError>>;

const LOG_TARGET: &'static str = "base_layer::p2p::services::liveness";

/// Initializer for the Liveness service handle and service future.
pub struct LivenessInitializer {
    inbound_message_subscription_factory: Arc<InboundTopicSubscriptionFactory<TariMessageType>>,
}

impl LivenessInitializer {
    /// Create a new LivenessInitializer from comms
    pub fn new(comms: Arc<CommsServices<TariMessageType>>) -> Self {
        Self {
            inbound_message_subscription_factory: comms.inbound_message_subscription_factory(),
        }
    }

    /// Create a new LivenessInitializer from the inbound message subscriber
    #[cfg(test)]
    pub fn inbound_message_subscription_factory(
        inbound_message_subscription_factory: Arc<InboundTopicSubscriptionFactory<TariMessageType>>,
    ) -> Self {
        Self {
            inbound_message_subscription_factory,
        }
    }

    /// Get a stream of inbound PingPong messages
    fn ping_stream(&self) -> impl Stream<Item = (MessageInfo, PingPong), Error = ()> {
        self.inbound_message_subscription_factory
            .get_subscription_compat(TariMessageType::new(NetMessage::PingPong))
            .and_then(|msg| {
                DomainMessageDeserializer::<PingPong>::new(msg).or_else(|_| {
                    error!(target: LOG_TARGET, "thread pool shut down");
                    future::err(())
                })
            })
            .filter_map(ok_or_skip_result)
    }
}

impl ServiceInitializer<ServiceName> for LivenessInitializer {
    fn initialize(self: Box<Self>, handles: ServiceHandlesFuture) -> Result<(), ServiceInitializationError> {
        let liveness_service = handles.lazy_service(move |handles| {
            // All handles are ready
            let state = Arc::new(LivenessState::new());
            let outbound_handle = handles
                .get_handle::<CommsOutboundHandle>(ServiceName::CommsOutbound)
                .expect("Liveness service requires CommsOutbound service handle");

            // Setup and start the inbound message handler
            let mut handler = LivenessHandler::new(Arc::clone(&state), outbound_handle.clone());
            let inbound_handler = self.ping_stream().for_each(move |(info, msg)| {
                handler.handle_message(info, msg).or_else(|err| {
                    error!("Error when processing message: {:?}", err);
                    future::err(())
                })
            });

            tokio::spawn(inbound_handler);

            LivenessService::new(state, outbound_handle)
        });

        let (requester, responder) = transport::channel(liveness_service);
        // Register handle and spawn the responder service
        handles.insert(ServiceName::Liveness, requester);
        tokio::spawn(responder);

        Ok(())
    }
}

fn ok_or_skip_result<T, E>(res: Result<T, E>) -> Option<T>
where E: Debug {
    match res {
        Ok(t) => Some(t),
        Err(err) => {
            tracing::error!(target: LOG_TARGET, "{:?}", err);
            None
        },
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn ok_or_skip_result() {
        let res = Result::<_, ()>::Ok(());
        assert_eq!(super::ok_or_skip_result(res).unwrap(), ());

        let res = Result::<(), _>::Err(());
        assert!(super::ok_or_skip_result(res).is_none());
    }
}
