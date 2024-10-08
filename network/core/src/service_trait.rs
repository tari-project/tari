// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use libp2p::{swarm::dial_opts::DialOpts, PeerId};
use tokio::sync::oneshot;

use crate::{messaging::MulticastDestination, MessageSpec, NetworkError};

#[async_trait]
pub trait NetworkingService {
    fn local_peer_id(&self) -> &PeerId;

    async fn dial_peer<T: Into<DialOpts> + Send + 'static>(&mut self, dial_opts: T)
        -> Result<Waiter<()>, NetworkError>;
    async fn disconnect_peer(&mut self, peer_id: PeerId) -> Result<bool, NetworkError>;

    async fn ban_peer<T: Into<String> + Send>(
        &mut self,
        peer_id: PeerId,
        reason: T,
        until: Option<Duration>,
    ) -> Result<bool, NetworkError>;
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
