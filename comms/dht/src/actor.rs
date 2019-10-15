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

//! Actor for DHT functionality.
//!
//! The DhtActor is responsible for sending a join request on startup
//! and furnishing [DhtRequest]s.
//!
//! [DhtRequest]: ./enum.DhtRequest.html

use crate::{
    envelope::{DhtMessageType, NodeDestination},
    inbound::{DiscoverMessage, JoinMessage},
    outbound::{
        BroadcastClosestRequest,
        BroadcastStrategy,
        DhtOutboundError,
        OutboundEncryption,
        OutboundMessageRequester,
    },
    store_forward::StoredMessagesRequest,
    DhtConfig,
};
use futures::{
    channel::{mpsc, mpsc::SendError},
    stream::Fuse,
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    peer_manager::{NodeId, NodeIdentity},
    types::CommsPublicKey,
};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &'static str = "comms::dht::actor";

pub enum DhtRequest {
    /// Send a Join request to the network
    SendJoin,
    /// Send a discover request for a network region or node
    SendDiscover {
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    },
}

pub struct DhtRequester {
    sender: mpsc::Sender<DhtRequest>,
}

impl DhtRequester {
    pub fn new(sender: mpsc::Sender<DhtRequest>) -> Self {
        Self { sender }
    }

    pub async fn send_join(&mut self) -> Result<(), SendError> {
        self.sender.send(DhtRequest::SendJoin).await
    }

    pub async fn send_discover(
        &mut self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<(), SendError>
    {
        self.sender
            .send(DhtRequest::SendDiscover {
                dest_public_key,
                dest_node_id,
                destination,
            })
            .await
    }
}

pub struct DhtActor {
    node_identity: Arc<NodeIdentity>,
    outbound_requester: OutboundMessageRequester,
    config: DhtConfig,
    shutdown_signal: Option<ShutdownSignal>,
    request_rx: Fuse<mpsc::Receiver<DhtRequest>>,
}

impl DhtActor {
    pub fn new(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<DhtRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            config,
            outbound_requester,
            node_identity,
            shutdown_signal: Some(shutdown_signal),
            request_rx: request_rx.fuse(),
        }
    }

    pub async fn start(mut self) {
        if self.config.enable_auto_join {
            match self.send_join().await {
                Ok(_) => {
                    trace!(target: LOG_TARGET, "Join message has been sent to closest peers",);
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send join message on startup because '{}'", err
                    );
                },
            }
        }

        if self.config.enable_auto_stored_message_request {
            match self.request_stored_messages().await {
                Ok(_) => {
                    trace!(
                        target: LOG_TARGET,
                        "Stored message request has been sent to closest peers",
                    );
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send stored message on startup because '{}'", err
                    );
                },
            }
        }

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("DhtActor initialized without shutdown_signal")
            .fuse();

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => {
                    self.handle_request(request).await;
                },

                _guard = shutdown_signal => {
                    info!(target: LOG_TARGET, "DHtActor is shutting down because it received a shutdown signal.");
                    break;
                },
                complete => {
                    info!(target: LOG_TARGET, "DHtActor is shutting down because the request stream ended.");
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, request: DhtRequest) {
        let result = match request {
            DhtRequest::SendJoin => self.send_join().await,
            DhtRequest::SendDiscover {
                destination,
                dest_node_id,
                dest_public_key,
            } => self.send_discover(dest_public_key, dest_node_id, destination).await,
        };

        match result {
            Ok(_) => {
                trace!(target: LOG_TARGET, "Successfully handled DHT request message");
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Error when handling DHT request message. {}", err);
            },
        }
    }

    async fn send_join(&mut self) -> Result<(), DhtOutboundError> {
        let message = JoinMessage {
            node_id: self.node_identity.identity.node_id.clone(),
            net_addresses: vec![self.node_identity.control_service_address()],
            peer_features: self.node_identity.features().clone(),
        };

        debug!(
            target: LOG_TARGET,
            "Sending Join message to (at most) {} closest peers", self.config.num_regional_nodes
        );

        self.outbound_requester
            .send_dht_message(
                BroadcastStrategy::Closest(BroadcastClosestRequest {
                    n: self.config.num_regional_nodes,
                    node_id: self.node_identity.identity.node_id.clone(),
                    excluded_peers: Vec::new(),
                }),
                NodeDestination::Undisclosed,
                OutboundEncryption::None,
                DhtMessageType::Join,
                message,
            )
            .await?;

        Ok(())
    }

    async fn send_discover(
        &mut self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<(), DhtOutboundError>
    {
        let discover_msg = DiscoverMessage {
            node_id: self.node_identity.identity.node_id.clone(),
            net_addresses: vec![self.node_identity.control_service_address()],
            peer_features: self.node_identity.features().clone(),
        };
        debug!(
            target: LOG_TARGET,
            "Sending Discover message to (at most) {} closest peers", self.config.num_regional_nodes
        );

        // If the destination node is is known, send to the closest peers we know. Otherwise...
        let network_location_node_id = dest_node_id.unwrap_or(match &destination {
            // ... if the destination is undisclosed or a public key, send discover to our closest peers
            NodeDestination::Undisclosed | NodeDestination::PublicKey(_) => self.node_identity.node_id().clone(),
            // otherwise, send it to the closest peers to the given NodeId destination we know
            NodeDestination::NodeId(node_id) => node_id.clone(),
        });

        let broadcast_strategy = BroadcastStrategy::Closest(BroadcastClosestRequest {
            n: self.config.num_regional_nodes,
            node_id: network_location_node_id,
            excluded_peers: Vec::new(),
        });

        self.outbound_requester
            .send_dht_message(
                broadcast_strategy,
                destination,
                OutboundEncryption::EncryptFor(dest_public_key),
                DhtMessageType::Discover,
                discover_msg,
            )
            .await?;

        Ok(())
    }

    async fn request_stored_messages(&mut self) -> Result<(), DhtOutboundError> {
        let broadcast_strategy = BroadcastStrategy::Closest(BroadcastClosestRequest {
            n: self.config.num_regional_nodes,
            node_id: self.node_identity.node_id().clone(),
            excluded_peers: Vec::new(),
        });

        self.outbound_requester
            .send_dht_message(
                broadcast_strategy,
                NodeDestination::Undisclosed,
                OutboundEncryption::EncryptForDestination,
                DhtMessageType::SAFRequestMessages,
                // TODO: We should track when this node last requested stored messages and ask
                //       for messages after that date
                StoredMessagesRequest::new(),
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::make_node_identity;
    use tari_shutdown::Shutdown;
    use tari_test_utils::runtime;

    #[test]
    fn auto_messages() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let (out_tx, mut out_rx) = mpsc::channel(1);
            let (_actor_tx, actor_rx) = mpsc::channel(1);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                DhtConfig::default(),
                node_identity,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::Join);
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::SAFRequestMessages);
            });
        });
    }

    #[test]
    fn send_join_request() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let (out_tx, mut out_rx) = mpsc::channel(1);
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let mut requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                DhtConfig {
                    enable_auto_join: false,
                    enable_auto_stored_message_request: false,
                    ..Default::default()
                },
                node_identity,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                requester.send_join().await.unwrap();
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::Join);
            });
        });
    }

    #[test]
    fn send_discover_request() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let (out_tx, mut out_rx) = mpsc::channel(1);
            let (actor_tx, actor_rx) = mpsc::channel(1);
            let mut requester = DhtRequester::new(actor_tx);
            let outbound_requester = OutboundMessageRequester::new(out_tx);
            let shutdown = Shutdown::new();
            let actor = DhtActor::new(
                DhtConfig {
                    enable_auto_join: false,
                    enable_auto_stored_message_request: false,
                    ..Default::default()
                },
                node_identity,
                outbound_requester,
                actor_rx,
                shutdown.to_signal(),
            );

            rt.spawn(actor.start());

            rt.block_on(async move {
                requester
                    .send_discover(CommsPublicKey::default(), None, NodeDestination::Undisclosed)
                    .await
                    .unwrap();
                let request = unwrap_oms_send_msg!(out_rx.next().await.unwrap());
                assert_eq!(request.dht_message_type, DhtMessageType::Discover);
            });
        });
    }
}
