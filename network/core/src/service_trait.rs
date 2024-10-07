// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use libp2p::{swarm::dial_opts::DialOpts, PeerId};
use tokio::sync::oneshot;

use crate::{messaging::MulticastDestination, GossipPublisher, GossipReceiver, MessageSpec, NetworkError};

#[async_trait]
pub trait NetworkingService {
    fn local_peer_id(&self) -> &PeerId;

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(&mut self, dial_opts: T)
        -> Result<Waiter<()>, NetworkError>;

    async fn get_connected_peers(&mut self) -> Result<Vec<PeerId>, NetworkError>;

    async fn publish_gossip<TTopic: Into<String> + Send>(
        &mut self,
        topic: TTopic,
        message: Vec<u8>,
    ) -> Result<(), NetworkError>;

    async fn subscribe_topic<T: Into<String> + Send, M: prost::Message + Default>(
        &mut self,
        topic: T,
    ) -> Result<(GossipPublisher<M>, GossipReceiver<M>), NetworkError>;
    async fn unsubscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkError>;

    async fn set_want_peers<I: IntoIterator<Item = PeerId> + Send>(&self, want_peers: I) -> Result<(), NetworkError>;

    async fn ban_peer(&mut self, peer_id: PeerId) -> Result<bool, NetworkError>;
}

pub trait OutboundMessager<TMsg: MessageSpec> {
    fn send_message<T: Into<TMsg::Message> + Send>(
        &mut self,
        peer: PeerId,
        message: T,
    ) -> impl Future<Output = Result<(), NetworkError>> + Send;

    /// Sends a message to the specified destination.
    /// Returns the number of messages that were successfully enqueued for sending.
    fn send_multicast<D: Into<MulticastDestination> + Send + 'static, T: Into<TMsg::Message> + Send>(
        &mut self,
        destination: D,
        message: T,
    ) -> impl Future<Output = Result<usize, NetworkError>> + Send;
}

pub struct Waiter<T> {
    rx: oneshot::Receiver<Result<T, NetworkError>>,
}

impl<T> From<oneshot::Receiver<Result<T, NetworkError>>> for Waiter<T> {
    fn from(rx: oneshot::Receiver<Result<T, NetworkError>>) -> Self {
        Self { rx }
    }
}

impl<T> Future for Waiter<T> {
    type Output = Result<T, NetworkError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().rx).poll(cx)?
    }
}
