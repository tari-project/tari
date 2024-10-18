// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tokio::sync::{mpsc, oneshot};

use crate::{
    error::NetworkingHandleError,
    handle::Reply,
    identity::PeerId,
    MessageSpec,
    NetworkError,
    OutboundMessager,
};

pub enum MessagingRequest<TMsg: MessageSpec> {
    SendMessage {
        peer: PeerId,
        message: TMsg::Message,
        reply_tx: Reply<()>,
    },
    SendMulticast {
        destination: MulticastDestination,
        message: TMsg::Message,
        reply_tx: Reply<usize>,
    },
}

#[derive(Debug)]
pub struct OutboundMessaging<TMsg: MessageSpec> {
    tx_request: mpsc::Sender<MessagingRequest<TMsg>>,
}

impl<TMsg: MessageSpec> OutboundMessaging<TMsg> {
    pub(crate) fn new(tx_request: mpsc::Sender<MessagingRequest<TMsg>>) -> Self {
        Self { tx_request }
    }
}

impl<TMsg: MessageSpec> OutboundMessager<TMsg> for OutboundMessaging<TMsg> {
    async fn send_message<T: Into<TMsg::Message> + Send>(
        &mut self,
        peer: PeerId,
        message: T,
    ) -> Result<(), NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(MessagingRequest::SendMessage {
                peer,
                message: message.into(),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }

    async fn send_multicast<D: Into<MulticastDestination> + Send, T: Into<TMsg::Message> + Send>(
        &mut self,
        dest: D,
        message: T,
    ) -> Result<usize, NetworkError> {
        let (tx, rx) = oneshot::channel();
        self.tx_request
            .send(MessagingRequest::SendMulticast {
                destination: dest.into(),
                message: message.into(),
                reply_tx: tx,
            })
            .await
            .map_err(|_| NetworkingHandleError::ServiceHasShutdown)?;
        rx.await?
    }
}

impl<TMsg: MessageSpec> Clone for OutboundMessaging<TMsg> {
    fn clone(&self) -> Self {
        OutboundMessaging {
            tx_request: self.tx_request.clone(),
        }
    }
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
        peers.iter().map(|p| **p).collect()
    }
}

impl IntoIterator for MulticastDestination {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = PeerId;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl FromIterator<PeerId> for MulticastDestination {
    fn from_iter<T: IntoIterator<Item = PeerId>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
