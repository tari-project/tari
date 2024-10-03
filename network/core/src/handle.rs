//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::collections::HashSet;

use async_trait::async_trait;
use libp2p::{gossipsub::IdentTopic, swarm::dial_opts::DialOpts, PeerId, StreamProtocol};
use log::*;
use tari_rpc_framework::{
    framing,
    framing::CanonicalFraming,
    NamedProtocolService,
    RpcClient,
    RpcClientBuilder,
    RpcConnector,
    Substream,
};
use tari_swarm::substream::{NegotiatedSubstream, ProtocolNotification};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    connection::Connection,
    error::NetworkingHandleError,
    event::NetworkingEvent,
    message::MessageSpec,
    peer::PeerInfo,
    NetworkingError,
    NetworkingService,
    Waiter,
};

const LOG_TARGET: &str = "tari::network::handle";

pub enum NetworkingRequest<TMsg: MessageSpec> {
    DialPeer {
        dial_opts: DialOpts,
        reply_tx: oneshot::Sender<Result<Waiter<()>, NetworkingError>>,
    },
    GetConnectedPeers {
        reply_tx: oneshot::Sender<Result<Vec<PeerId>, NetworkingError>>,
    },
    SendMessage {
        peer: PeerId,
        message: TMsg::Message,
        reply_tx: oneshot::Sender<Result<(), NetworkingError>>,
    },
    SendMulticast {
        destination: MulticastDestination,
        message: TMsg::Message,
        reply_tx: oneshot::Sender<Result<usize, NetworkingError>>,
    },
    PublishGossip {
        topic: IdentTopic,
        message: TMsg::GossipMessage,
        reply_tx: oneshot::Sender<Result<(), NetworkingError>>,
    },
    SubscribeTopic {
        topic: IdentTopic,
        reply_tx: oneshot::Sender<Result<(), NetworkingError>>,
    },
    UnsubscribeTopic {
        topic: IdentTopic,
        reply_tx: oneshot::Sender<Result<(), NetworkingError>>,
    },
    IsSubscribedTopic {
        topic: IdentTopic,
        reply_tx: oneshot::Sender<Result<bool, NetworkingError>>,
    },
    OpenSubstream {
        peer_id: PeerId,
        protocol_id: StreamProtocol,
        reply_tx: oneshot::Sender<Result<NegotiatedSubstream<Substream>, NetworkingError>>,
    },
    AddProtocolNotifier {
        protocols: HashSet<StreamProtocol>,
        tx_notifier: mpsc::UnboundedSender<ProtocolNotification<Substream>>,
    },
    GetActiveConnections {
        reply_tx: oneshot::Sender<Result<Vec<Connection>, NetworkingError>>,
    },
    GetLocalPeerInfo {
        reply_tx: oneshot::Sender<Result<PeerInfo, NetworkingError>>,
    },
    SetWantPeers(HashSet<PeerId>),
}

#[derive(Debug, Clone, Default)]
pub struct MulticastDestination(Vec<PeerId>);

impl MulticastDestination {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, peer: PeerId) {
        self.0.push(peer);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Vec<PeerId>> for MulticastDestination {
    fn from(peers: Vec<PeerId>) -> Self {
        Self(peers)
    }
}

impl From<&[PeerId]> for MulticastDestination {
    fn from(peers: &[PeerId]) -> Self {
        peers.to_vec().into()
    }
}

impl From<Vec<&PeerId>> for MulticastDestination {
    fn from(peers: Vec<&PeerId>) -> Self {
        peers[..].to_vec().into()
    }
}

impl IntoIterator for MulticastDestination {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = PeerId;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug)]
pub struct NetworkingHandle<TMsg: MessageSpec> {
    tx_request: mpsc::Sender<NetworkingRequest<TMsg>>,
    local_peer_id: PeerId,
    tx_events: broadcast::Sender<NetworkingEvent>,
}

impl<TMsg: MessageSpec> NetworkingHandle<TMsg> {
    pub(super) fn new(
        local_peer_id: PeerId,
        tx_request: mpsc::Sender<NetworkingRequest<TMsg>>,
        tx_events: broadcast::Sender<NetworkingEvent>,
    ) -> Self {
        Self {
            tx_request,
            local_peer_id,
            tx_events,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<NetworkingEvent> {
        self.tx_events.subscribe()
    }

    pub async fn is_subscribed_to_topic<T: Into<String>>(&self, topic: T) -> Result<bool, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::IsSubscribedTopic {
                topic: IdentTopic::new(topic),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    /// Add a notifier for these protocols. An unbounded sender is used to prevent potential lockups waiting for
    /// consumers to read the notification.
    pub async fn add_protocol_notifier<I: IntoIterator<Item = StreamProtocol>>(
        &mut self,
        protocols: I,
        tx_notifier: mpsc::UnboundedSender<ProtocolNotification<Substream>>,
    ) -> Result<(), NetworkingError> {
        self.tx_request
            .send(NetworkingRequest::AddProtocolNotifier {
                protocols: protocols.into_iter().collect(),
                tx_notifier,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        Ok(())
    }

    pub async fn open_substream(
        &mut self,
        peer_id: PeerId,
        protocol_id: &StreamProtocol,
    ) -> Result<NegotiatedSubstream<Substream>, NetworkingError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::OpenSubstream {
                peer_id,
                protocol_id: protocol_id.clone(),
                reply_tx,
            })
            .await?;
        reply_rx
            .await
            .map_err(|_| NetworkingHandleError::ServiceAbandonedRequest)?
    }

    pub async fn open_framed_substream(
        &mut self,
        peer_id: PeerId,
        protocol_id: &StreamProtocol,
        max_frame_size: usize,
    ) -> Result<CanonicalFraming<Substream>, NetworkingError> {
        let substream = self.open_substream(peer_id, protocol_id).await?;
        Ok(framing::canonical(substream.stream, max_frame_size))
    }

    pub async fn get_active_connections(&self) -> Result<Vec<Connection>, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetActiveConnections { reply_tx: tx })
            .await?;
        rx.await?
    }

    pub async fn get_local_peer_info(&self) -> Result<PeerInfo, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetLocalPeerInfo { reply_tx: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }
}

#[async_trait]
impl<TMsg: MessageSpec + Send + 'static> NetworkingService<TMsg> for NetworkingHandle<TMsg> {
    fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(
        &mut self,
        dial_opts: T,
    ) -> Result<Waiter<()>, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::DialPeer {
                dial_opts: dial_opts.into(),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn get_connected_peers(&mut self) -> Result<Vec<PeerId>, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetConnectedPeers { reply_tx: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn send_message(&mut self, peer: PeerId, message: TMsg::Message) -> Result<(), NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::SendMessage {
                peer,
                message,
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn send_multicast<D: Into<MulticastDestination> + Send>(
        &mut self,
        dest: D,
        message: TMsg::Message,
    ) -> Result<usize, NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::SendMulticast {
                destination: dest.into(),
                message,
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn publish_gossip<TTopic: Into<String> + Send>(
        &mut self,
        topic: TTopic,
        message: TMsg::GossipMessage,
    ) -> Result<(), NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::PublishGossip {
                topic: IdentTopic::new(topic),
                message,
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn subscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::SubscribeTopic {
                topic: IdentTopic::new(topic),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn unsubscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkingError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::UnsubscribeTopic {
                topic: IdentTopic::new(topic),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn set_want_peers<I: IntoIterator<Item = PeerId> + Send>(
        &self,
        want_peers: I,
    ) -> Result<(), NetworkingError> {
        let want_peers = want_peers.into_iter().collect();
        self.tx_request
            .send(NetworkingRequest::SetWantPeers(want_peers))
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        Ok(())
    }
}

impl<TMsg: MessageSpec> Clone for NetworkingHandle<TMsg> {
    fn clone(&self) -> Self {
        Self {
            tx_request: self.tx_request.clone(),
            local_peer_id: self.local_peer_id,
            tx_events: self.tx_events.clone(),
        }
    }
}

impl<TMsg> RpcConnector for NetworkingHandle<TMsg> {
    type Error = NetworkingError;

    async fn is_connected(&self, peer_id: &PeerId) -> Result<bool, Self::Error> {}

    async fn connect_rpc_using_builder<T>(&mut self, builder: RpcClientBuilder<T>) -> Result<T, Self::Error>
    where T: From<RpcClient> + NamedProtocolService {
        let protocol = StreamProtocol::new(T::PROTOCOL_NAME);
        debug!(
            target: LOG_TARGET,
            "Attempting to establish RPC protocol `{}` to peer `{}`",
            protocol,
            builder.peer_id()
        );
        let framed = self
            .open_framed_substream(*builder.peer_id(), &protocol, tari_rpc_framework::RPC_MAX_FRAME_SIZE)
            .await?;
        let client = builder.with_protocol_id(protocol).connect(framed).await?;
        Ok(client)
    }
}
