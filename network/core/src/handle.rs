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
use libp2p::{gossipsub, gossipsub::IdentTopic, swarm::dial_opts::DialOpts, PeerId, StreamProtocol};
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
    peer::{Peer, PeerInfo},
    GossipPublisher,
    GossipReceiver,
    NetworkError,
    NetworkingService,
    ReachabilityMode,
    Waiter,
};

const LOG_TARGET: &str = "tari::network::handle";

pub(super) type Reply<T> = oneshot::Sender<Result<T, NetworkError>>;

pub enum NetworkingRequest {
    DialPeer {
        dial_opts: DialOpts,
        reply_tx: Reply<Waiter<()>>,
    },
    GetConnectedPeers {
        reply_tx: Reply<Vec<PeerId>>,
    },

    PublishGossip {
        topic: IdentTopic,
        message: Vec<u8>,
        reply_tx: Reply<()>,
    },
    SubscribeTopic {
        topic: IdentTopic,
        inbound: mpsc::UnboundedSender<(PeerId, gossipsub::Message)>,
        reply: Reply<mpsc::Sender<(IdentTopic, Vec<u8>)>>,
    },
    UnsubscribeTopic {
        topic: IdentTopic,
        reply_tx: Reply<()>,
    },
    IsSubscribedTopic {
        topic: IdentTopic,
        reply_tx: Reply<bool>,
    },
    OpenSubstream {
        peer_id: PeerId,
        protocol_id: StreamProtocol,
        reply_tx: Reply<NegotiatedSubstream<Substream>>,
    },
    AddProtocolNotifier {
        protocols: HashSet<StreamProtocol>,
        tx_notifier: mpsc::UnboundedSender<ProtocolNotification<Substream>>,
    },
    SelectActiveConnections {
        limit: Option<usize>,
        randomize: bool,
        exclude_peers: HashSet<PeerId>,
        reply_tx: Reply<Vec<Connection>>,
    },
    GetLocalPeerInfo {
        reply_tx: Reply<PeerInfo>,
    },
    SetWantPeers(HashSet<PeerId>),
    AddPeer {
        peer: Peer,
        reply: Reply<()>,
    },
    BanPeer {
        peer_id: PeerId,
        reply: Reply<bool>,
    },
}
#[derive(Debug)]
pub struct NetworkingHandle {
    tx_request: mpsc::Sender<NetworkingRequest>,
    local_peer_id: PeerId,
    tx_events: broadcast::Sender<NetworkingEvent>,
}

impl NetworkingHandle {
    pub(super) fn new(
        local_peer_id: PeerId,
        tx_request: mpsc::Sender<NetworkingRequest>,
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

    pub async fn is_subscribed_to_topic<T: Into<String>>(&self, topic: T) -> Result<bool, NetworkError> {
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
    ) -> Result<(), NetworkError> {
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
    ) -> Result<NegotiatedSubstream<Substream>, NetworkError> {
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
    ) -> Result<CanonicalFraming<Substream>, NetworkError> {
        let substream = self.open_substream(peer_id, protocol_id).await?;
        Ok(framing::canonical(substream.stream, max_frame_size))
    }

    pub async fn get_active_connections(&self) -> Result<Vec<Connection>, NetworkError> {
        self.select_active_connections(None, false, HashSet::new()).await
    }

    pub async fn select_active_connections(
        &self,
        limit: Option<usize>,
        randomize: bool,
        exclude_peers: HashSet<PeerId>,
    ) -> Result<Vec<Connection>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::SelectActiveConnections {
                limit,
                randomize,
                exclude_peers,
                reply_tx: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn get_local_peer_info(&self) -> Result<PeerInfo, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetLocalPeerInfo { reply_tx: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn add_peer(&self, peer: Peer) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::AddPeer { peer, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }
}

#[async_trait]
impl NetworkingService for NetworkingHandle {
    fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(
        &mut self,
        dial_opts: T,
    ) -> Result<Waiter<()>, NetworkError> {
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

    async fn get_connected_peers(&mut self) -> Result<Vec<PeerId>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetConnectedPeers { reply_tx: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn publish_gossip<TTopic: Into<String> + Send>(
        &mut self,
        topic: TTopic,
        message: Vec<u8>,
    ) -> Result<(), NetworkError> {
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

    async fn subscribe_topic<T: Into<String> + Send, M: prost::Message + Default>(
        &mut self,
        topic: T,
    ) -> Result<(GossipPublisher<M>, GossipReceiver<M>), NetworkError> {
        let (inbound, receiver) = mpsc::unbounded_channel();

        let (tx, rx) = oneshot::channel();
        let topic = IdentTopic::new(topic);

        self.tx_request
            .send(NetworkingRequest::SubscribeTopic {
                topic: topic.clone(),
                inbound,
                reply: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        let sender = rx.await??;

        Ok((
            GossipPublisher::<M>::new(topic, sender),
            GossipReceiver::<M>::new(receiver),
        ))
    }

    async fn unsubscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkError> {
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

    async fn set_want_peers<I: IntoIterator<Item = PeerId> + Send>(&self, want_peers: I) -> Result<(), NetworkError> {
        let want_peers = want_peers.into_iter().collect();
        self.tx_request
            .send(NetworkingRequest::SetWantPeers(want_peers))
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        Ok(())
    }

    async fn ban_peer(&mut self, peer_id: PeerId) -> Result<bool, NetworkError> {
        let (reply, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::BanPeer { peer_id, reply })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;

        rx.await?
    }
}

impl Clone for NetworkingHandle {
    fn clone(&self) -> Self {
        Self {
            tx_request: self.tx_request.clone(),
            local_peer_id: self.local_peer_id,
            tx_events: self.tx_events.clone(),
        }
    }
}

impl RpcConnector for NetworkingHandle {
    type Error = NetworkError;

    async fn connect_rpc_using_builder<T>(&mut self, builder: RpcClientBuilder<T>) -> Result<T, Self::Error>
    where T: From<RpcClient> + NamedProtocolService + Send {
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
