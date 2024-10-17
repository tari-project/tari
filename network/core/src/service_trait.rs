// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use libp2p::{swarm::dial_opts::DialOpts, PeerId};
use tokio::sync::oneshot;

use crate::{error::DialError, messaging::MulticastDestination, MessageSpec, NetworkError};

pub trait NetworkingService {
    fn local_peer_id(&self) -> &PeerId;

    fn dial_peer<T: Into<DialOpts> + Send + 'static>(
        &mut self,
        dial_opts: T,
    ) -> impl Future<Output = Result<DialWaiter<()>, NetworkError>> + Send;
    fn disconnect_peer(&mut self, peer_id: PeerId) -> impl Future<Output = Result<bool, NetworkError>> + Send;

    fn ban_peer<T: Into<String> + Send>(
        &mut self,
        peer_id: PeerId,
        reason: T,
        until: Option<Duration>,
    ) -> impl Future<Output = Result<bool, NetworkError>> + Send;

    fn unban_peer(&mut self, peer_id: PeerId) -> impl Future<Output = Result<bool, NetworkError>> + Send;
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

pub struct DialWaiter<T> {
    rx: oneshot::Receiver<Result<T, DialError>>,
}

impl<T> From<oneshot::Receiver<Result<T, DialError>>> for DialWaiter<T> {
    fn from(rx: oneshot::Receiver<Result<T, DialError>>) -> Self {
        Self { rx }
    }
}

impl<T> Future for DialWaiter<T> {
    type Output = Result<T, DialError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().rx)
            .poll(cx)
            .map_err(|_| DialError::ServiceHasShutDown)?
    }
}

pub struct Waiter<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> From<oneshot::Receiver<T>> for Waiter<T> {
    fn from(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Future for Waiter<T> {
    type Output = Result<T, oneshot::error::RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().rx).poll(cx)
    }
}
