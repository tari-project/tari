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
    outbound::{BroadcastClosestRequest, BroadcastStrategy, OutboundMessageRequester},
    store_forward::error::StoreAndForwardError,
    DhtConfig,
};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{
    peer_manager::{NodeIdentity, PeerManager},
    types::CommsPublicKey,
};
use tari_comms_middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::store_forward::forward";

/// This layer is responsible for forwarding messages which have failed to decrypt
pub struct ForwardLayer {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundMessageRequester,
}

impl ForwardLayer {
    pub fn new(
        peer_manager: Arc<PeerManager>,
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundMessageRequester,
    ) -> Self
    {
        Self {
            peer_manager,
            config,
            node_identity,
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
            self.config.num_regional_nodes,
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
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
    config_num_regional_nodes: usize,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundMessageRequester,
}

impl<S> ForwardMiddleware<S> {
    pub fn new(
        service: S,
        config_num_regional_nodes: usize,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundMessageRequester,
    ) -> Self
    {
        Self {
            next_service: service,
            config_num_regional_nodes,
            peer_manager,
            node_identity,
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
            self.config_num_regional_nodes,
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            self.outbound_service.clone(),
        )
        .handle(msg)
    }
}

/// Responsible for processing a single DecryptedDhtMessage, forwarding if necessary or passing the message
/// to the next service.
struct Forwarder<S> {
    peer_manager: Arc<PeerManager>,
    config_num_regional_nodes: usize,
    node_identity: Arc<NodeIdentity>,
    next_service: S,
    outbound_service: OutboundMessageRequester,
}

impl<S> Forwarder<S> {
    pub fn new(
        service: S,
        config_num_regional_nodes: usize,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundMessageRequester,
    ) -> Self
    {
        Self {
            peer_manager,
            config_num_regional_nodes,
            node_identity,
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
            comms_header,
            decryption_result,
            dht_header,
            ..
        } = message;

        let body = decryption_result
            .clone()
            .err()
            .expect("previous check that decryption failed");

        let broadcast_strategy = self.get_broadcast_strategy(dht_header.destination.clone(), vec![comms_header
            .message_public_key
            .clone()])?;

        self.outbound_service
            .forward_message(broadcast_strategy, dht_header.clone(), body)
            .await?;

        Ok(())
    }

    /// Selects the most appropriate broadcast strategy based on the received messages destination
    fn get_broadcast_strategy(
        &self,
        header_dest: NodeDestination,
        excluded_peers: Vec<CommsPublicKey>,
    ) -> Result<BroadcastStrategy, StoreAndForwardError>
    {
        let this_node_id = self.node_identity.node_id();
        Ok(match header_dest {
            NodeDestination::Undisclosed => {
                // Send to the current nodes nearest neighbours
                BroadcastStrategy::Closest(BroadcastClosestRequest {
                    n: self.config_num_regional_nodes,
                    node_id: this_node_id.clone(),
                    excluded_peers,
                })
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if self.peer_manager.exists(&dest_public_key)? {
                    // Send to destination peer directly if the current node knows that peer
                    BroadcastStrategy::DirectPublicKey(dest_public_key)
                } else {
                    // Send to the current nodes nearest neighbours
                    BroadcastStrategy::Closest(BroadcastClosestRequest {
                        n: self.config_num_regional_nodes,
                        node_id: this_node_id.clone(),
                        excluded_peers,
                    })
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                match self.peer_manager.find_with_node_id(&dest_node_id) {
                    Ok(dest_peer) => {
                        // Send to destination peer directly if the current node knows that peer
                        BroadcastStrategy::DirectPublicKey(dest_peer.public_key)
                    },
                    Err(_) => {
                        // Send to peers that are closest to the destination network region
                        BroadcastStrategy::Closest(BroadcastClosestRequest {
                            n: self.config_num_regional_nodes,
                            node_id: dest_node_id,
                            excluded_peers,
                        })
                    },
                }
            },
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        outbound::DhtOutboundRequest,
        test_utils::{make_dht_inbound_message, make_node_identity, make_peer_manager, service_spy},
    };
    use futures::{channel::mpsc, executor::block_on, StreamExt};
    use tari_comms::message::Message;

    #[test]
    fn decryption_succeeded() {
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let (oms_tx, mut oms_rx) = mpsc::channel(1);
        let oms = OutboundMessageRequester::new(oms_tx);
        let mut service = ForwardLayer::new(peer_manager, DhtConfig::default(), node_identity, oms)
            .layer(spy.service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::succeeded(Message::from_message_format((), ()).unwrap(), inbound_msg);
        block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());
        assert!(oms_rx.try_next().is_err());
    }

    #[test]
    fn decryption_failed() {
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let (oms_tx, mut oms_rx) = mpsc::channel(1);
        let oms = OutboundMessageRequester::new(oms_tx);
        let mut service = ForwardLayer::new(peer_manager, DhtConfig::default(), node_identity, oms)
            .layer(spy.service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::failed(inbound_msg);
        block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());
        let oms_req = block_on(oms_rx.next()).unwrap();

        match oms_req {
            DhtOutboundRequest::Forward(_) => {},
            _ => panic!("Unexpected OMS request"),
        }
    }
}
