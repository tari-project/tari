//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use libp2p::{swarm::dial_opts::DialOpts, PeerId};
use tokio::sync::oneshot;

mod worker;

mod error;
pub use error::NetworkingError;

mod config;
mod connection;
mod event;
mod global_ip;
mod handle;
mod message;
mod notify;
mod peer;
mod relay_state;
mod spawn;

pub use config::*;
pub use connection::*;
pub use handle::*;
pub use message::*;
pub use spawn::*;
pub use tari_swarm::{
    config::{Config as SwarmConfig, LimitPerInterval, RelayCircuitLimits, RelayReservationLimits},
    is_supported_multiaddr,
};

#[async_trait]
pub trait NetworkingService<TMsg: MessageSpec> {
    fn local_peer_id(&self) -> &PeerId;

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(
        &mut self,
        dial_opts: T,
    ) -> Result<Waiter<()>, NetworkingError>;

    async fn get_connected_peers(&mut self) -> Result<Vec<PeerId>, NetworkingError>;

    async fn send_message(&mut self, peer: PeerId, message: TMsg::Message) -> Result<(), NetworkingError>;

    /// Sends a message to the specified destination.
    /// Returns the number of messages that were successfully enqueued for sending.
    async fn send_multicast<D: Into<MulticastDestination> + Send + 'static>(
        &mut self,
        destination: D,
        message: TMsg::Message,
    ) -> Result<usize, NetworkingError>;

    async fn publish_gossip<TTopic: Into<String> + Send>(
        &mut self,
        topic: TTopic,
        message: TMsg::GossipMessage,
    ) -> Result<(), NetworkingError>;

    async fn subscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkingError>;
    async fn unsubscribe_topic<T: Into<String> + Send>(&mut self, topic: T) -> Result<(), NetworkingError>;

    async fn set_want_peers<I: IntoIterator<Item = PeerId> + Send>(&self, want_peers: I)
        -> Result<(), NetworkingError>;
}

pub struct Waiter<T> {
    rx: oneshot::Receiver<Result<T, NetworkingError>>,
}

impl<T> From<oneshot::Receiver<Result<T, NetworkingError>>> for Waiter<T> {
    fn from(rx: oneshot::Receiver<Result<T, NetworkingError>>) -> Self {
        Self { rx }
    }
}

impl<T> Future for Waiter<T> {
    type Output = Result<T, NetworkingError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().rx).poll(cx)?
    }
}
