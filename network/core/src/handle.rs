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

use std::{collections::HashSet, time::Duration};

use libp2p::{gossipsub, gossipsub::IdentTopic, swarm::dial_opts::DialOpts, Multiaddr, PeerId, StreamProtocol};
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
    event::NetworkEvent,
    peer::{Peer, PeerInfo},
    BannedPeer,
    DialWaiter,
    DiscoveryResult,
    GossipPublisher,
    GossipSubscription,
    NetworkError,
    NetworkingService,
    Waiter,
};

const LOG_TARGET: &str = "network::handle";

pub(super) type Reply<T> = oneshot::Sender<Result<T, NetworkError>>;

pub enum NetworkingRequest {
    DialPeer {
        dial_opts: DialOpts,
        reply_tx: Reply<DialWaiter<()>>,
    },
    DisconnectPeer {
        peer_id: PeerId,
        reply: Reply<bool>,
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
        with_peers: Option<HashSet<PeerId>>,
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
        reason: String,
        ban_duration: Option<Duration>,
        reply: Reply<bool>,
    },
    UnbanPeer {
        peer_id: PeerId,
        reply: Reply<bool>,
    },
    GetKnownPeerAddresses {
        peer_id: PeerId,
        reply: Reply<Option<Vec<Multiaddr>>>,
    },
    GetBannedPeer {
        peer_id: PeerId,
        reply: Reply<Option<BannedPeer>>,
    },
    GetBannedPeers {
        reply: Reply<Vec<BannedPeer>>,
    },
    AddPeerToAllowList {
        peer_id: PeerId,
        reply: Reply<()>,
    },
    RemovePeerFromAllowList {
        peer_id: PeerId,
        reply: Reply<bool>,
    },
    DiscoverClosestPeers {
        peer_id: PeerId,
        reply: Reply<Waiter<DiscoveryResult>>,
    },
    GetSeedPeers {
        reply: Reply<Vec<Peer>>,
    },
}
#[derive(Debug)]
pub struct NetworkHandle {
    tx_request: mpsc::Sender<NetworkingRequest>,
    local_peer_id: PeerId,
    tx_events: broadcast::Sender<NetworkEvent>,
}

impl NetworkHandle {
    pub(super) fn new(
        local_peer_id: PeerId,
        tx_request: mpsc::Sender<NetworkingRequest>,
        tx_events: broadcast::Sender<NetworkEvent>,
    ) -> Self {
        Self {
            tx_request,
            local_peer_id,
            tx_events,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<NetworkEvent> {
        self.tx_events.subscribe()
    }

    pub async fn get_seed_peers(&self) -> Result<Vec<Peer>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetSeedPeers { reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
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
        &self,
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
        &self,
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
        &self,
        peer_id: PeerId,
        protocol_id: &StreamProtocol,
        max_frame_size: usize,
    ) -> Result<CanonicalFraming<Substream>, NetworkError> {
        let substream = self.open_substream(peer_id, protocol_id).await?;
        Ok(framing::canonical(substream.stream, max_frame_size))
    }

    pub async fn get_active_connections(&self) -> Result<Vec<Connection>, NetworkError> {
        self.select_active_connections(None, None, false, HashSet::new()).await
    }

    pub async fn get_connection(&self, peer_id: PeerId) -> Result<Option<Connection>, NetworkError> {
        let mut set = HashSet::new();
        set.insert(peer_id);
        let mut conns = self
            .select_active_connections(Some(set), Some(1), false, HashSet::new())
            .await?;
        Ok(conns.pop())
    }

    pub async fn select_random_connections(
        &self,
        n: usize,
        exclude_peers: HashSet<PeerId>,
    ) -> Result<Vec<Connection>, NetworkError> {
        self.select_active_connections(None, Some(n), true, exclude_peers).await
    }

    pub async fn select_active_connections(
        &self,
        with_peers: Option<HashSet<PeerId>>,
        limit: Option<usize>,
        randomize: bool,
        exclude_peers: HashSet<PeerId>,
    ) -> Result<Vec<Connection>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::SelectActiveConnections {
                with_peers,
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

    pub async fn get_known_peer_addresses(&self, peer_id: PeerId) -> Result<Option<Vec<Multiaddr>>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetKnownPeerAddresses { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn publish_gossip<TTopic: Into<String> + Send>(
        &self,
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

    pub async fn subscribe_topic<T: Into<String> + Send, M: prost::Message + Default>(
        &self,
        topic: T,
    ) -> Result<(GossipPublisher<M>, GossipSubscription<M>), NetworkError> {
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
            GossipSubscription::<M>::new(receiver),
        ))
    }

    pub async fn unsubscribe_topic<T: Into<String> + Send>(&self, topic: T) -> Result<(), NetworkError> {
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

    pub async fn set_want_peers<I: IntoIterator<Item = PeerId> + Send>(
        &self,
        want_peers: I,
    ) -> Result<(), NetworkError> {
        let want_peers = want_peers.into_iter().collect();
        self.tx_request
            .send(NetworkingRequest::SetWantPeers(want_peers))
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        Ok(())
    }

    pub async fn get_banned_peer(&self, peer_id: PeerId) -> Result<Option<BannedPeer>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetBannedPeer { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn get_banned_peers(&self) -> Result<Vec<BannedPeer>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::GetBannedPeers { reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn add_peer_to_allow_list(&self, peer_id: PeerId) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::AddPeerToAllowList { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn remove_peer_from_allow_list(&self, peer_id: PeerId) -> Result<bool, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::RemovePeerFromAllowList { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn discover_peer(&self, peer_id: PeerId) -> Result<Waiter<DiscoveryResult>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::DiscoverClosestPeers { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    pub async fn wait_until_shutdown(&self) {
        self.tx_request.closed().await;
    }
}

impl NetworkingService for NetworkHandle {
    fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(
        &mut self,
        dial_opts: T,
    ) -> Result<DialWaiter<()>, NetworkError> {
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

    /// Disconnects a peer. Returns true if the peer was connected, otherwise false.
    async fn disconnect_peer(&mut self, peer_id: PeerId) -> Result<bool, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::DisconnectPeer { peer_id, reply: tx })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn ban_peer<T: Into<String> + Send>(
        &mut self,
        peer_id: PeerId,
        reason: T,
        ban_duration: Option<Duration>,
    ) -> Result<bool, NetworkError> {
        let (reply, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::BanPeer {
                peer_id,
                reason: reason.into(),
                ban_duration,
                reply,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;

        rx.await?
    }

    async fn unban_peer(&mut self, peer_id: PeerId) -> Result<bool, NetworkError> {
        let (reply, rx) = oneshot::channel();
        self.tx_request
            .send(NetworkingRequest::UnbanPeer { peer_id, reply })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;

        rx.await?
    }
}

impl Clone for NetworkHandle {
    fn clone(&self) -> Self {
        Self {
            tx_request: self.tx_request.clone(),
            local_peer_id: self.local_peer_id,
            tx_events: self.tx_events.clone(),
        }
    }
}

impl RpcConnector for NetworkHandle {
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
