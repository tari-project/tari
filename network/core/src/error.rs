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

use std::io;

use libp2p::{gossipsub, gossipsub::SubscriptionError, Multiaddr, TransportError};
use tari_rpc_framework::RpcError;
use tari_swarm::{messaging, substream, TariSwarmError};
use tokio::sync::{mpsc, oneshot};

use crate::{
    identity::PeerId,
    swarm,
    swarm::{derive_prelude::ConnectedPoint, dial_opts},
};

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Codec IO error: {0}")]
    CodecError(io::Error),
    #[error("Gossipsub publish error: {0}")]
    GossipPublishError(#[from] gossipsub::PublishError),
    #[error("Failed to send message to peer: {0}")]
    SwarmError(#[from] TariSwarmError),
    #[error("Failed to invoke handle: {0}")]
    NetworkingHandleError(#[from] NetworkingHandleError),
    #[error("Failed to subscribe to topic: {0}")]
    SubscriptionError(#[from] SubscriptionError),
    #[error("Dial failed: {0}")]
    DialError(#[from] swarm::DialError),
    #[error("Messaging error: {0}")]
    MessagingError(#[from] messaging::Error),
    #[error("Failed to open substream: {0}")]
    FailedToOpenSubstream(#[from] substream::Error),
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
    #[error("Transport error: {0}")]
    TransportError(#[from] TransportError<io::Error>),
    #[error("Peer sync error: {0}")]
    PeerSyncError(#[from] tari_swarm::peersync::Error),
    #[error("Messaging is disabled")]
    MessagingDisabled,
    #[error("Failed to add peer: {details}")]
    FailedToAddPeer { details: String },
}

impl From<oneshot::error::RecvError> for NetworkError {
    fn from(e: oneshot::error::RecvError) -> Self {
        Self::NetworkingHandleError(e.into())
    }
}

impl<T> From<mpsc::error::SendError<T>> for NetworkError {
    fn from(e: mpsc::error::SendError<T>) -> Self {
        Self::NetworkingHandleError(e.into())
    }
}

/// This is a mirror of libp2p DialError that we can:
/// 1. clone - needed for service responses
/// 2. impl thiserror
#[derive(Debug, thiserror::Error, Clone)]
pub enum DialError {
    #[error("The peer identity obtained on the connection matches the local peer.")]
    LocalPeerId { endpoint: ConnectedPoint },
    #[error(
        "No addresses have been provided by [`NetworkBehaviour::handle_pending_outbound_connection`] and [`DialOpts`]."
    )]
    NoAddresses,
    #[error("The provided [`dial_opts::PeerCondition`] {0:?} evaluated to false and thus the dial was aborted.")]
    DialPeerConditionFalse(dial_opts::PeerCondition),
    #[error("Pending connection attempt has been aborted.")]
    Aborted,
    #[error("The peer identity obtained ({obtained}) on the connection did not match the one that was expected.")]
    WrongPeerId { obtained: PeerId, endpoint: ConnectedPoint },
    #[error("One of the [`NetworkBehaviour`]s rejected the outbound connection: {cause}.")]
    Denied { cause: String },
    #[error("An error occurred while negotiating the transport protocol(s) on a connection. {}", .0.iter().map(|(a, err)| format!("{a}: {err}")).collect::<Vec<_>>().join(""))]
    Transport(Vec<(Multiaddr, String)>),
    #[error("Internal service was shutdown before the new connection could be established")]
    ServiceHasShutDown,
}

impl From<swarm::DialError> for DialError {
    fn from(value: swarm::DialError) -> Self {
        match value {
            swarm::DialError::LocalPeerId { endpoint } => DialError::LocalPeerId { endpoint },
            swarm::DialError::NoAddresses => DialError::NoAddresses,
            swarm::DialError::DialPeerConditionFalse(cond) => DialError::DialPeerConditionFalse(cond),
            swarm::DialError::Aborted => DialError::Aborted,
            swarm::DialError::WrongPeerId { obtained, endpoint } => DialError::WrongPeerId { obtained, endpoint },
            swarm::DialError::Denied { cause } => DialError::Denied {
                cause: cause.to_string(),
            },
            swarm::DialError::Transport(errs) => DialError::Transport(
                errs.into_iter()
                    .map(|(addr, err)| match err {
                        err @ TransportError::MultiaddrNotSupported(_) => (addr, err.to_string()),
                        TransportError::Other(err) => (addr, err.to_string()),
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkingHandleError {
    #[error("Service has shutdown")]
    ServiceHasShutdown,
    #[error("Service dropped reply sender without sending a reply")]
    ServiceAbandonedRequest,
}

impl From<oneshot::error::RecvError> for NetworkingHandleError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self::ServiceAbandonedRequest
    }
}

impl<T> From<mpsc::error::SendError<T>> for NetworkingHandleError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self::ServiceHasShutdown
    }
}
