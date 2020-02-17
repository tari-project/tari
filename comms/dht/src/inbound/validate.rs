// Copyright 2019, The Tari Project
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
    inbound::DhtInboundMessage,
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::{
        dht::{RejectMessage, RejectMessageReason},
        envelope::{DhtMessageType, Network},
    },
    PipelineError,
};
use futures::{task::Context, Future};
use log::*;
use std::task::Poll;
use tari_comms::message::MessageExt;
use tari_crypto::tari_utilities::ByteArray;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::validate";

/// # DHT validation middleware
///
/// Takes in a `DhtInboundMessage` and checks the message header for any invalid fields
/// If an invalid message is detected a rejection message is sent to the sending peer.
#[derive(Clone)]
pub struct ValidateMiddleware<S> {
    next_service: S,
    target_network: Network,
    outbound_requester: OutboundMessageRequester,
}

impl<S> ValidateMiddleware<S> {
    pub fn new(service: S, target_network: Network, outbound_requester: OutboundMessageRequester) -> Self {
        Self {
            next_service: service,
            target_network,
            outbound_requester,
        }
    }
}

impl<S> Service<DhtInboundMessage> for ValidateMiddleware<S>
where
    S: Service<DhtInboundMessage, Response = ()> + Clone + 'static,
    S::Error: Into<PipelineError>,
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtInboundMessage) -> Self::Future {
        Self::process_message(
            self.next_service.clone(),
            self.target_network,
            self.outbound_requester.clone(),
            msg,
        )
    }
}

impl<S> ValidateMiddleware<S>
where
    S: Service<DhtInboundMessage, Response = ()>,
    S::Error: Into<PipelineError>,
{
    pub async fn process_message(
        next_service: S,
        target_network: Network,
        mut outbound_requester: OutboundMessageRequester,
        message: DhtInboundMessage,
    ) -> Result<(), PipelineError>
    {
        trace!(
            target: LOG_TARGET,
            "Checking the message target network is '{:?}'",
            target_network
        );
        if message.dht_header.network == target_network {
            next_service.oneshot(message).await.map_err(Into::into)?;
        } else {
            debug!(
                target: LOG_TARGET,
                "Message is for another network (want = {:?} got = {:?}). Explicitly rejecting the message.",
                target_network,
                message.dht_header.network
            );
            outbound_requester
                .send_raw(
                    SendMessageParams::new()
                        .direct_public_key(message.source_peer.public_key)
                        .with_dht_message_type(DhtMessageType::RejectMsg)
                        .finish(),
                    RejectMessage {
                        signature: message
                            .dht_header
                            .origin
                            .map(|o| o.public_key.to_vec())
                            .unwrap_or_default(),
                        reason: RejectMessageReason::UnsupportedNetwork as i32,
                    }
                    .to_encoded_bytes()?,
                )
                .await?;
        }

        Ok(())
    }
}

pub struct ValidateLayer {
    target_network: Network,
    outbound_requester: OutboundMessageRequester,
}

impl ValidateLayer {
    pub fn new(target_network: Network, outbound_requester: OutboundMessageRequester) -> Self {
        Self {
            target_network,
            outbound_requester,
        }
    }
}

impl<S> Layer<S> for ValidateLayer {
    type Service = ValidateMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        ValidateMiddleware::new(service, self.target_network, self.outbound_requester.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::{DhtMessageFlags, DhtMessageType},
        outbound::mock::create_outbound_service_mock,
        test_utils::{make_dht_inbound_message, make_node_identity, service_spy},
    };
    use tari_test_utils::panic_context;
    use tokio::runtime::Runtime;

    #[test]
    fn process_message() {
        let mut rt = Runtime::new().unwrap();
        let spy = service_spy();

        let (out_requester, mock) = create_outbound_service_mock(1);
        let mock_state = mock.get_state();
        rt.spawn(mock.run());

        let mut validate =
            ValidateLayer::new(Network::LocalTest, out_requester).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(validate.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let mut msg = make_dht_inbound_message(&node_identity, Vec::new(), DhtMessageFlags::empty());
        msg.dht_header.network = Network::MainNet;

        rt.block_on(validate.call(msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 0);

        msg.dht_header.network = Network::LocalTest;

        rt.block_on(validate.call(msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 1);

        let calls = mock_state.take_calls();
        assert_eq!(calls.len(), 1);
        let params = calls[0].0.clone();
        assert_eq!(params.dht_message_type, DhtMessageType::RejectMsg);
        assert_eq!(
            params.broadcast_strategy.direct_public_key().unwrap(),
            node_identity.public_key()
        );

        // Drop validate so that the mock will stop running
        drop(validate);
    }
}
