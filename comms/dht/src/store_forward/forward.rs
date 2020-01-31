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
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    outbound::{OutboundMessageRequester, SendMessageParams},
    store_forward::error::StoreAndForwardError,
};
use futures::{task::Context, Future};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::{middleware::MiddlewareError, peer_manager::PeerManager, types::CommsPublicKey};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::store_forward::forward";

/// This layer is responsible for forwarding messages which have failed to decrypt
pub struct ForwardLayer {
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
}

impl ForwardLayer {
    pub fn new(peer_manager: Arc<PeerManager>, outbound_service: OutboundMessageRequester) -> Self {
        Self {
            peer_manager,
            outbound_service,
        }
    }
}

impl<S> Layer<S> for ForwardLayer {
    type Service = ForwardMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        ForwardMiddleware::new(
            service,
            // Pass in just the config item needed by the middleware for almost free copies
            Arc::clone(&self.peer_manager),
            self.outbound_service.clone(),
        )
    }
}

/// # Forward middleware
///
/// Responsible for forwarding messages which fail to decrypt.
#[derive(Clone)]
pub struct ForwardMiddleware<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
}

impl<S> ForwardMiddleware<S> {
    pub fn new(service: S, peer_manager: Arc<PeerManager>, outbound_service: OutboundMessageRequester) -> Self {
        Self {
            next_service: service,
            peer_manager,
            outbound_service,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for ForwardMiddleware<S>
where
    S: Service<DecryptedDhtMessage, Response = ()> + Clone + 'static,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        Forwarder::new(
            self.next_service.clone(),
            Arc::clone(&self.peer_manager),
            self.outbound_service.clone(),
        )
        .handle(msg)
    }
}

/// Responsible for processing a single DecryptedDhtMessage, forwarding if necessary or passing the message
/// to the next service.
struct Forwarder<S> {
    peer_manager: Arc<PeerManager>,
    next_service: S,
    outbound_service: OutboundMessageRequester,
}

impl<S> Forwarder<S> {
    pub fn new(service: S, peer_manager: Arc<PeerManager>, outbound_service: OutboundMessageRequester) -> Self {
        Self {
            peer_manager,
            next_service: service,
            outbound_service,
        }
    }
}

impl<S> Forwarder<S>
where
    S: Service<DecryptedDhtMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), MiddlewareError> {
        if message.decryption_failed() {
            debug!(target: LOG_TARGET, "Decryption failed. Forwarding message");
            self.forward(&message).await?;
        }

        // The message has been forwarded, but other middleware may be interested (i.e. StoreMiddleware)
        trace!(target: LOG_TARGET, "Passing message to next service");
        self.next_service.oneshot(message).await.map_err(Into::into)?;
        Ok(())
    }

    async fn forward(&mut self, message: &DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        let DecryptedDhtMessage {
            source_peer,
            decryption_result,
            dht_header,
            ..
        } = message;

        let body = decryption_result
            .clone()
            .err()
            .expect("previous check that decryption failed");

        let mut message_params =
            self.get_send_params(dht_header.destination.clone(), vec![source_peer.public_key.clone()])?;

        message_params.with_dht_header(dht_header.clone());

        self.outbound_service.send_raw(message_params.finish(), body).await?;

        Ok(())
    }

    /// Selects the most appropriate broadcast strategy based on the received messages destination
    fn get_send_params(
        &self,
        header_dest: NodeDestination,
        excluded_peers: Vec<CommsPublicKey>,
    ) -> Result<SendMessageParams, StoreAndForwardError>
    {
        let mut params = SendMessageParams::new();
        match header_dest {
            NodeDestination::Unknown => {
                // Send to the current nodes nearest neighbours
                params.neighbours(excluded_peers);
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if self.peer_manager.exists(&dest_public_key) {
                    // Send to destination peer directly if the current node knows that peer
                    params.direct_public_key(dest_public_key);
                } else {
                    // Send to the current nodes nearest neighbours
                    params.neighbours(excluded_peers);
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                match self.peer_manager.find_by_node_id(&dest_node_id) {
                    Ok(dest_peer) => {
                        // Send to destination peer directly if the current node knows that peer
                        params.direct_public_key(dest_peer.public_key);
                    },
                    Err(_) => {
                        // Send to peers that are closest to the destination network region
                        params.neighbours(excluded_peers);
                    },
                }
            },
        }

        Ok(params)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        outbound::mock::create_outbound_service_mock,
        test_utils::{make_dht_inbound_message, make_node_identity, make_peer_manager, service_spy},
    };
    use futures::{channel::mpsc, executor::block_on};
    use tari_comms::wrap_in_envelope_body;
    use tokio::runtime::Runtime;

    #[test]
    fn decryption_succeeded() {
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let (oms_tx, mut oms_rx) = mpsc::channel(1);
        let oms = OutboundMessageRequester::new(oms_tx);
        let mut service = ForwardLayer::new(peer_manager, oms).layer(spy.to_service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(Vec::new()).unwrap(), inbound_msg);
        block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());
        assert!(oms_rx.try_next().is_err());
    }

    #[test]
    fn decryption_failed() {
        let mut rt = Runtime::new().unwrap();
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let (oms_requester, oms_mock) = create_outbound_service_mock(1);
        let oms_mock_state = oms_mock.get_state();
        rt.spawn(oms_mock.run());

        let mut service = ForwardLayer::new(peer_manager, oms_requester).layer(spy.to_service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::failed(inbound_msg);
        rt.block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());

        assert_eq!(oms_mock_state.call_count(), 1);
        let (params, _) = oms_mock_state.pop_call().unwrap();

        assert!(params.dht_header.is_some());
    }
}
