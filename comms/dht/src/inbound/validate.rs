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

use crate::{inbound::DhtInboundMessage, proto::envelope::Network};
use futures::{task::Context, Future};
use log::*;
use std::task::Poll;
use tari_comms::pipeline::PipelineError;
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
}

impl<S> ValidateMiddleware<S> {
    pub fn new(service: S, target_network: Network) -> Self {
        Self {
            next_service: service,
            target_network,
        }
    }
}

impl<S> Service<DhtInboundMessage> for ValidateMiddleware<S>
where S: Service<DhtInboundMessage, Response = (), Error = PipelineError> + Clone + 'static
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DhtInboundMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let target_network = self.target_network;
        async move {
            if message.dht_header.network == target_network && message.dht_header.is_valid() {
                debug!(
                    target: LOG_TARGET,
                    "Passing message {} to next service (Trace: {})", message.tag, message.dht_header.message_tag
                );
                next_service.oneshot(message).await?;
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Message is for another network (want = {:?} got = {:?}) or message header is invalid. Discarding \
                     the message (Trace: {}).",
                    target_network,
                    message.dht_header.network,
                    message.dht_header.message_tag
                );
            }

            Ok(())
        }
    }
}

pub struct ValidateLayer {
    target_network: Network,
}

impl ValidateLayer {
    pub fn new(target_network: Network) -> Self {
        Self { target_network }
    }
}

impl<S> Layer<S> for ValidateLayer {
    type Service = ValidateMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        ValidateMiddleware::new(service, self.target_network)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_node_identity, service_spy},
    };
    use tari_test_utils::panic_context;
    use tokio::runtime::Runtime;

    #[test]
    fn process_message() {
        let mut rt = Runtime::new().unwrap();
        let spy = service_spy();

        let mut validate = ValidateLayer::new(Network::LocalTest).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(validate.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let mut msg = make_dht_inbound_message(&node_identity, Vec::new(), DhtMessageFlags::empty(), false);
        msg.dht_header.network = Network::MainNet;

        rt.block_on(validate.call(msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 0);

        msg.dht_header.network = Network::LocalTest;

        rt.block_on(validate.call(msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 1);
    }
}
