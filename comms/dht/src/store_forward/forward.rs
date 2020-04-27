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
    envelope::{DhtMessageHeader, NodeDestination},
    inbound::DecryptedDhtMessage,
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::envelope::DhtMessageType,
    store_forward::error::StoreAndForwardError,
};
use futures::{task::Context, Future};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    peer_manager::{Peer, PeerManager},
    pipeline::PipelineError,
    types::CommsPublicKey,
};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::storeforward::forward";

/// This layer is responsible for forwarding messages which have failed to decrypt
pub struct ForwardLayer {
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    is_enabled: bool,
}

impl ForwardLayer {
    pub fn new(peer_manager: Arc<PeerManager>, outbound_service: OutboundMessageRequester, is_enabled: bool) -> Self {
        Self {
            peer_manager,
            outbound_service,
            is_enabled,
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
            self.is_enabled,
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
    is_enabled: bool,
}

impl<S> ForwardMiddleware<S> {
    pub fn new(
        service: S,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
        is_enabled: bool,
    ) -> Self
    {
        Self {
            next_service: service,
            peer_manager,
            outbound_service,
            is_enabled,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for ForwardMiddleware<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + 'static
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DecryptedDhtMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let peer_manager = Arc::clone(&self.peer_manager);
        let outbound_service = self.outbound_service.clone();
        let is_enabled = self.is_enabled;
        async move {
            if !is_enabled {
                trace!(target: LOG_TARGET, "Passing message to next service (Not enabled)");
                return next_service.oneshot(message).await;
            }

            let forwarder = Forwarder::new(next_service, peer_manager, outbound_service);
            forwarder.handle(message).await
        }
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
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), PipelineError> {
        if message.decryption_failed() {
            debug!(target: LOG_TARGET, "Decryption failed. Forwarding message");
            self.forward(&message).await.map_err(PipelineError::from_debug)?;
        }

        // The message has been forwarded, but other middleware may be interested (i.e. StoreMiddleware)
        trace!(target: LOG_TARGET, "Passing message to next service");
        self.next_service.oneshot(message).await?;
        Ok(())
    }

    async fn forward(&mut self, message: &DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        let DecryptedDhtMessage {
            source_peer,
            decryption_result,
            dht_header,
            authenticated_origin,
            ..
        } = message;

        if self.destination_matches_source(&dht_header.destination, &source_peer) {
            // TODO: #banheuristic - the origin of this message was the destination. Two things are wrong here:
            //       1. The origin/destination should not have forwarded this (the destination node didnt do
            //          this destination_matches_source check)
            //       1. The source sent a message that the destination could not decrypt
            //       The authenticated source should be banned (malicious), and origin should be temporarily banned
            //       (bug?)
            warn!(
                target: LOG_TARGET,
                "Received message from peer '{}' that is destined for that peer. Discarding message",
                source_peer.node_id.short_str()
            );
            return Ok(());
        }

        let body = decryption_result
            .clone()
            .err()
            .expect("previous check that decryption failed");

        let mut excluded_peers = vec![source_peer.public_key.clone()];
        if let Some(pk) = authenticated_origin.as_ref() {
            excluded_peers.push(pk.clone());
        }

        let mut message_params = self.get_send_params(&dht_header, excluded_peers).await?;
        message_params.with_dht_header(dht_header.clone());

        self.outbound_service.send_raw(message_params.finish(), body).await?;

        Ok(())
    }

    /// Selects the most appropriate broadcast strategy based on the received messages destination
    async fn get_send_params(
        &self,
        header: &DhtMessageHeader,
        excluded_peers: Vec<CommsPublicKey>,
    ) -> Result<SendMessageParams, StoreAndForwardError>
    {
        let mut params = SendMessageParams::new();
        // If this is a DHT Discovery message, forward this message to our closest communication node and _all_ known
        // communication clients
        let is_discovery = header.message_type == DhtMessageType::Discovery;

        match header.destination.clone() {
            NodeDestination::Unknown => {
                // Send to the current nodes nearest neighbours
                if is_discovery {
                    params.neighbours_include_clients(excluded_peers);
                } else {
                    params.neighbours(excluded_peers);
                }
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if self.peer_manager.exists(&dest_public_key).await {
                    // Send to destination peer directly if the current node knows that peer
                    params.direct_public_key(*dest_public_key);
                } else {
                    // Send to the current nodes nearest neighbours
                    if is_discovery {
                        params.neighbours_include_clients(excluded_peers);
                    } else {
                        params.neighbours(excluded_peers);
                    }
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                match self.peer_manager.find_by_node_id(&dest_node_id).await {
                    Ok(dest_peer) => {
                        // Send to destination peer directly if the current node knows that peer
                        params.direct_public_key(dest_peer.public_key);
                    },
                    Err(_) => {
                        // Send to peers that are closest to the destination network region
                        if is_discovery {
                            params.neighbours_include_clients(excluded_peers);
                        } else {
                            params.neighbours(excluded_peers);
                        }
                    },
                }
            },
        }

        Ok(params)
    }

    fn destination_matches_source(&self, destination: &NodeDestination, source: &Peer) -> bool {
        if let Some(pk) = destination.public_key() {
            return pk == &source.public_key;
        }

        if let Some(node_id) = destination.node_id() {
            return node_id == &source.node_id;
        }

        false
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
        let mut service = ForwardLayer::new(peer_manager, oms, true).layer(spy.to_service::<PipelineError>());

        let node_identity = make_node_identity();
        let inbound_msg = make_dht_inbound_message(&node_identity, b"".to_vec(), DhtMessageFlags::empty(), false);
        let msg = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(Vec::new()),
            Some(node_identity.public_key().clone()),
            inbound_msg,
        );
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

        let mut service = ForwardLayer::new(peer_manager, oms_requester, true).layer(spy.to_service::<PipelineError>());

        let sample_body = b"Lorem ipsum";
        let inbound_msg = make_dht_inbound_message(
            &make_node_identity(),
            sample_body.to_vec(),
            DhtMessageFlags::empty(),
            false,
        );
        let header = inbound_msg.dht_header.clone();
        let msg = DecryptedDhtMessage::failed(inbound_msg);
        rt.block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());

        assert_eq!(oms_mock_state.call_count(), 1);
        let (params, body) = oms_mock_state.pop_call().unwrap();

        // Header and body are preserved when forwarding
        assert_eq!(&body.to_vec(), &sample_body);
        assert_eq!(params.dht_header.unwrap(), header);
    }
}
